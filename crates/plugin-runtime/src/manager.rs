use std::collections::HashMap;
use std::sync::Arc;

use ora_contracts::acp::notification::{CancelNotification, SessionNotification};
use ora_contracts::acp::prompt::{PromptRequest, PromptResponse};
use ora_contracts::acp::session::{NewSessionRequest, NewSessionResponse};
use ora_contracts::{
    InitializeRequest, InitializeResponse, PluginProcessEntrypoint, plugin_methods,
};
use ora_process::{ManagedProcess, ProcessSpawner, ProcessSpec};
use tokio::sync::{Mutex, broadcast};
use tokio_util::sync::CancellationToken;

use crate::channel::PluginChannel;
use crate::error::PluginRuntimeError;

/// Plugin-channel protocol version spoken by the host during the `initialize` handshake.
const PLUGIN_CHANNEL_VERSION: &str = "0.1.0";

/// One active plugin: its channel, the owning process handle, and a cancellation token.
struct ActivePlugin<P: ManagedProcess> {
    channel: Arc<PluginChannel>,
    process: Mutex<Option<P>>,
    #[allow(dead_code)]
    token: CancellationToken,
}

/// Owns the live runtime for activated plugins: spawns plugin processes, drives the
/// plugin-channel handshake, and forwards agent operations to the right plugin by id.
///
/// Generic over the [`ProcessSpawner`] so tests can inject a fake spawner without starting
/// real child processes. Event notifications are delivered through a single broadcast channel
/// that adapters (Tauri, Web) forward to their transports.
pub struct PluginRuntimeManager<Spawner>
where
    Spawner: ProcessSpawner,
{
    spawner: Spawner,
    sessions: Mutex<HashMap<String, ActivePlugin<Spawner::Process>>>,
    notifications: broadcast::Sender<SessionNotification>,
}

impl<Spawner> PluginRuntimeManager<Spawner>
where
    Spawner: ProcessSpawner + Send + Sync + 'static,
    Spawner::Process: ManagedProcess + Send + 'static,
{
    /// Builds a manager with the supplied spawner and a fresh notification broadcast.
    pub fn new(spawner: Spawner) -> Self {
        let (notifications, _) = broadcast::channel(128);
        Self {
            spawner,
            sessions: Mutex::new(HashMap::new()),
            notifications,
        }
    }

    /// Returns a receiver for `agent/sessionUpdate` notifications from every active plugin.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionNotification> {
        self.notifications.subscribe()
    }

    /// Spawns a plugin process, completes the `initialize` handshake, and caches the channel.
    ///
    /// After this returns the plugin is in the `Started` state; the first `new_session` call
    /// promotes it to `Activated`.
    pub async fn activate(
        &self,
        plugin_id: &str,
        entrypoint: PluginProcessEntrypoint,
        source_path: &str,
    ) -> Result<(), PluginRuntimeError> {
        let mut sessions = self.sessions.lock().await;
        if sessions.contains_key(plugin_id) {
            return Err(PluginRuntimeError::AlreadyActive {
                plugin_id: plugin_id.to_string(),
            });
        }

        let spec = build_process_spec(&entrypoint, source_path);
        let mut process = self
            .spawner
            .spawn(spec)
            .map_err(|error| PluginRuntimeError::Spawn {
                message: error.to_string(),
            })?;

        let stdin = process
            .take_stdin()
            .ok_or_else(|| PluginRuntimeError::Spawn {
                message: "plugin stdin pipe unavailable".to_string(),
            })?;
        let stdout = process
            .take_stdout()
            .ok_or_else(|| PluginRuntimeError::Spawn {
                message: "plugin stdout pipe unavailable".to_string(),
            })?;

        let channel = PluginChannel::new(
            Box::new(stdin),
            Box::new(stdout),
            self.notifications.clone(),
        );

        let _response: InitializeResponse = channel
            .request(
                plugin_methods::INITIALIZE,
                InitializeRequest {
                    protocol_version: PLUGIN_CHANNEL_VERSION.to_string(),
                },
            )
            .await?;

        let active = ActivePlugin {
            channel,
            process: Mutex::new(Some(process)),
            token: CancellationToken::new(),
        };
        sessions.insert(plugin_id.to_string(), active);
        Ok(())
    }

    /// Sends a graceful shutdown and terminates the plugin process, removing it from the cache.
    pub async fn deactivate(&self, plugin_id: &str) -> Result<(), PluginRuntimeError> {
        let mut sessions = self.sessions.lock().await;
        let Some(active) = sessions.remove(plugin_id) else {
            return Err(PluginRuntimeError::NotActive {
                plugin_id: plugin_id.to_string(),
            });
        };
        drop(sessions);

        if let Some(process) = active.process.lock().await.take() {
            let _ = process.kill().await;
        }
        Ok(())
    }

    /// Forwards an `agent/newSession` request to the active plugin.
    pub async fn new_session(
        &self,
        plugin_id: &str,
        request: NewSessionRequest,
    ) -> Result<NewSessionResponse, PluginRuntimeError> {
        let channel = self.channel_for(plugin_id).await?;
        channel
            .request(plugin_methods::AGENT_NEW_SESSION, request)
            .await
    }

    /// Forwards an `agent/prompt` request to the active plugin.
    /// `agent/sessionUpdate` notifications stream through [`subscribe`] before the response resolves.
    pub async fn prompt(
        &self,
        plugin_id: &str,
        request: PromptRequest,
    ) -> Result<PromptResponse, PluginRuntimeError> {
        let channel = self.channel_for(plugin_id).await?;
        channel.request(plugin_methods::AGENT_PROMPT, request).await
    }

    /// Forwards an `agent/cancel` notification-equivalent to the active plugin.
    pub async fn cancel(
        &self,
        plugin_id: &str,
        notification: CancelNotification,
    ) -> Result<(), PluginRuntimeError> {
        let channel = self.channel_for(plugin_id).await?;
        channel
            .request(plugin_methods::AGENT_CANCEL, notification)
            .await
    }

    async fn channel_for(&self, plugin_id: &str) -> Result<Arc<PluginChannel>, PluginRuntimeError> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(plugin_id)
            .map(|active| active.channel.clone())
            .ok_or_else(|| PluginRuntimeError::NotActive {
                plugin_id: plugin_id.to_string(),
            })
    }
}

/// Builds the process spawn spec from the manifest entrypoint. The working directory
/// defaults to the plugin's source path so manifest args can be relative to the plugin dir.
///
/// On Windows a bare command name (e.g. `tsx`, `pnpm`, `node`) is usually a `.cmd` shim that
/// `std::process::Command` cannot resolve by itself; such bare names are routed through
/// `cmd /c`, which resolves `.cmd`/`.bat` via PATHEXT. Names containing a path separator or
/// extension are spawned directly.
fn build_process_spec(entrypoint: &PluginProcessEntrypoint, source_path: &str) -> ProcessSpec {
    let bare_windows_shim = cfg!(windows)
        && !entrypoint
            .program
            .chars()
            .any(|c| matches!(c, '/' | '\\' | '.'));

    let mut spec = if bare_windows_shim {
        let mut wrapped = ProcessSpec::new("cmd");
        wrapped = wrapped.arg("/c").arg(&entrypoint.program);
        wrapped
    } else {
        ProcessSpec::new(&entrypoint.program)
    };
    for arg in &entrypoint.args {
        spec = spec.arg(arg);
    }
    spec = spec.cwd(entrypoint.cwd.as_deref().unwrap_or(source_path));
    for (key, value) in &entrypoint.envs {
        spec = spec.env(key, value);
    }
    spec
}

#[cfg(test)]
mod end_to_end_tests {
    use super::*;
    use ora_contracts::PluginProcessEntrypoint;
    use ora_contracts::acp::notification::CancelNotification;
    use ora_contracts::acp::prompt::{PromptRequest, StopReason};
    use ora_contracts::acp::session::NewSessionRequest;
    use ora_process::TokioProcessSpawner;
    use std::path::PathBuf;
    use std::time::Duration;

    /// Spawns the real mock-agent plugin (via pnpm) and drives the full runtime: activate →
    /// newSession → prompt (streaming session updates through the broadcast) → cancel → deactivate.
    ///
    /// Ignored by default because it shells out to `pnpm`; run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "spawns the real mock-agent plugin via pnpm; run with --ignored"]
    async fn end_to_end_with_real_mock_agent() {
        let manager = PluginRuntimeManager::new(TokioProcessSpawner::new());
        // Bare `pnpm` exercises build_process_spec's Windows `.cmd`-shim routing (cmd /c).
        let entrypoint = PluginProcessEntrypoint {
            program: "pnpm".to_string(),
            args: vec![
                "--filter".into(),
                "@ora-plugins/mock-agent".into(),
                "exec".into(),
                "tsx".into(),
                "src/adapter.ts".into(),
            ],
            cwd: None,
            envs: Vec::new(),
        };
        manager
            .activate("mock-agent", entrypoint, ".")
            .await
            .expect("activate spawns the mock-agent and completes the initialize handshake");

        let session = manager
            .new_session("mock-agent", NewSessionRequest::new(PathBuf::from(".")))
            .await
            .expect("new_session");
        let session_id = session.session_id.0.as_ref().to_string();
        assert_eq!(session_id, "mock-session-1");

        // Subscribe before prompting so streaming session updates are captured.
        let mut receiver = manager.subscribe();
        let response = manager
            .prompt(
                "mock-agent",
                PromptRequest::new(session_id.clone(), Vec::new()),
            )
            .await
            .expect("prompt");
        assert_eq!(response.stop_reason, StopReason::EndTurn);

        // The mock-agent streams two agent_message_chunk updates before ending the turn.
        let mut updates = 0;
        while let Ok(Ok(_)) =
            tokio::time::timeout(Duration::from_millis(300), receiver.recv()).await
        {
            updates += 1;
        }
        assert!(
            updates >= 2,
            "expected at least 2 session updates, got {updates}"
        );

        manager
            .cancel("mock-agent", CancelNotification::new(session_id))
            .await
            .expect("cancel");
        manager.deactivate("mock-agent").await.expect("deactivate");
    }
}
