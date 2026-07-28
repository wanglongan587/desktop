use crate::config::DesktopConfigStore;
use ora_backend::Backend;
use ora_plugin_runtime::PluginRuntimeManager;
use ora_process::TokioProcessSpawner;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Holds the shared Backend, Desktop configuration store, and live plugin runtime managed by Tauri.
#[derive(Clone)]
pub struct DesktopState {
    pub backend: Backend,
    pub config: DesktopConfigStore,
    pub stream_cancellations: Arc<Mutex<HashMap<String, CancellationToken>>>,
    pub plugin_runtime: Arc<PluginRuntimeManager<TokioProcessSpawner>>,
}

/// Retains process-scoped writer guards for the full Tauri application lifetime.
pub struct DesktopRuntimeGuard {
    pub _logging: ora_logging::LoggingGuard,
}
