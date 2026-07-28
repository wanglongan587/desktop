use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Top-level kind of a plugin; selects the host-side runtime that drives it.
///
/// New kinds are added by extending this enum and implementing the corresponding
/// `PluginRuntime`; the installable manifest shape stays unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub enum PluginKind {
    /// A plugin that bridges the host to an external agent over ACP.
    Agent,
    /// Reserved for future plugins that contribute UI surfaces.
    #[allow(dead_code)]
    Ui,
    /// Reserved for future plugins that contribute workbench features.
    #[allow(dead_code)]
    Workbench,
}

/// Serializable spawn configuration for the plugin process.
///
/// The host interprets this to spawn the plugin process; it is distinct from any
/// agent process the plugin itself spawns internally (Model B: the plugin owns ACP).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct PluginProcessEntrypoint {
    /// Executable path or name passed to the OS to spawn the plugin process.
    pub program: String,
    /// Command-line arguments in insertion order.
    #[serde(default)]
    pub args: Vec<String>,
    /// Optional working directory for the plugin process; omitted from the manifest when unset.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cwd: Option<String>,
    /// Environment overrides applied to the plugin process, in insertion order.
    #[serde(default)]
    pub envs: Vec<(String, String)>,
}

/// Cross-kind descriptor of an installable plugin.
///
/// Doubles as the on-disk manifest format scanned from a plugin directory and the
/// wire shape exchanged with the frontend. Launch data for any agent the plugin
/// bridges to is owned by the plugin itself, not by this manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct PluginManifest {
    /// Stable plugin identifier; also the key the runtime caches plugin handles by.
    pub id: String,
    /// Plugin manifest version, semver-style string.
    pub version: String,
    /// Selects the host-side runtime that drives this plugin.
    pub kind: PluginKind,
    /// How the host spawns the plugin process.
    pub entrypoint: PluginProcessEntrypoint,
    /// Human-readable name shown in management surfaces.
    pub display_name: String,
    /// Human-readable description shown in management surfaces.
    pub description: String,
}

/// Mirrors `ora_domain::PluginLifecycleState`; the persisted view of where a plugin
/// sits in its F1 lifecycle (see ADR-0001). Kept separate so contracts stays
/// independent of the domain crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub enum PluginState {
    Discovered,
    Installed,
    Enabled,
    Started,
    Activated,
}

/// Managed (persisted) plugin view exposed to management surfaces.
///
/// The persisted manifest plus the current lifecycle state and source path. Distinct
/// from [`PluginManifest`] which describes only the installable artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct Plugin {
    pub id: String,
    pub version: String,
    pub kind: PluginKind,
    pub entrypoint: PluginProcessEntrypoint,
    pub display_name: String,
    pub description: String,
    pub state: PluginState,
    pub source_path: String,
}

/// One plugin discovered by scanning a plugins directory, before it is installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct DiscoveredPlugin {
    pub manifest: PluginManifest,
    pub source_path: String,
}

/// Host → plugin handshake request establishing the plugin channel.
///
/// Completing the handshake transitions the plugin to the `Started` lifecycle state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct InitializeRequest {
    /// Plugin-channel protocol version the host speaks.
    pub protocol_version: String,
}

/// Plugin → host handshake response confirming readiness and declared kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct InitializeResponse {
    /// The manifest kind this plugin process serves.
    pub kind: PluginKind,
    /// Echo of the manifest version.
    pub version: String,
}

/// Requests scanning the plugins directory for installable manifests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct ScanPluginsRequest {}

/// Returns every plugin discovered by the scan, before installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct ScanPluginsResponse {
    pub plugins: Vec<DiscoveredPlugin>,
}

/// Requests installing a discovered plugin by passing back its scanned manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct InstallPluginRequest {
    pub plugin: DiscoveredPlugin,
}

/// Returns the plugin that was just installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct InstallPluginResponse {
    pub plugin: Plugin,
}

/// Requests every installed plugin in deterministic storage order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct ListPluginsRequest {}

/// Returns every installed plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct ListPluginsResponse {
    pub plugins: Vec<Plugin>,
}

/// Requests enabling an installed plugin by identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct EnablePluginRequest {
    pub plugin_id: String,
}

/// Returns the plugin whose enabled transition completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct EnablePluginResponse {
    pub plugin: Plugin,
}

/// Requests disabling an enabled (or running) plugin by identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct DisablePluginRequest {
    pub plugin_id: String,
}

/// Returns the plugin whose disable transition completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct DisablePluginResponse {
    pub plugin: Plugin,
}

/// Requests uninstalling (soft-deleting) a plugin by identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct UninstallPluginRequest {
    pub plugin_id: String,
}

/// Returns the identifier of the plugin that was uninstalled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct UninstallPluginResponse {
    pub plugin_id: String,
}

/// Plugin-channel JSON-RPC method names.
///
/// The agent-kind methods reuse ACP payload types as their params/results, but keep
/// an `agent/*` method namespace so the plugin-channel contract is not coupled to
/// ACP's wire format (see ADR-0001).
pub mod plugin_methods {
    /// Handshake; completes the `Started` lifecycle state.
    pub const INITIALIZE: &str = "initialize";

    /// Opens an agent session; completes the `Activated` lifecycle state.
    /// Params: `NewSessionRequest`; result: `NewSessionResponse`.
    pub const AGENT_NEW_SESSION: &str = "agent/newSession";

    /// Sends a prompt turn; streams `AGENT_SESSION_UPDATE` notifications before the response.
    /// Params: `PromptRequest`; result: `PromptResponse`.
    pub const AGENT_PROMPT: &str = "agent/prompt";

    /// Cancels the in-flight prompt for a session.
    /// Params: `CancelNotification`.
    pub const AGENT_CANCEL: &str = "agent/cancel";

    /// Requests graceful plugin process shutdown; returns to the `Enabled` state.
    pub const SHUTDOWN: &str = "shutdown";

    /// Plugin → host notification carrying an ACP session update.
    /// Params: `SessionNotification`.
    pub const AGENT_SESSION_UPDATE: &str = "agent/sessionUpdate";
}

/// Exports every TypeScript binding declared in this module into the target directory.
pub(crate) fn export(config: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    PluginKind::export(config)?;
    PluginProcessEntrypoint::export(config)?;
    PluginManifest::export(config)?;
    PluginState::export(config)?;
    Plugin::export(config)?;
    DiscoveredPlugin::export(config)?;
    InitializeRequest::export(config)?;
    InitializeResponse::export(config)?;
    ScanPluginsRequest::export(config)?;
    ScanPluginsResponse::export(config)?;
    InstallPluginRequest::export(config)?;
    InstallPluginResponse::export(config)?;
    ListPluginsRequest::export(config)?;
    ListPluginsResponse::export(config)?;
    EnablePluginRequest::export(config)?;
    EnablePluginResponse::export(config)?;
    DisablePluginRequest::export(config)?;
    DisablePluginResponse::export(config)?;
    UninstallPluginRequest::export(config)?;
    UninstallPluginResponse::export(config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveredPlugin, InitializeRequest, InitializeResponse, Plugin, PluginKind,
        PluginManifest, PluginProcessEntrypoint, PluginState,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn sample_entrypoint() -> PluginProcessEntrypoint {
        PluginProcessEntrypoint {
            program: "node".to_string(),
            args: vec!["plugins/codex/adapter.js".to_string()],
            cwd: None,
            envs: vec![("CODEX_PATH".to_string(), "/usr/bin/codex".to_string())],
        }
    }

    fn sample_manifest() -> PluginManifest {
        PluginManifest {
            id: "codex".to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::Agent,
            entrypoint: sample_entrypoint(),
            display_name: "Codex".to_string(),
            description: "Bridges Ora to the codex agent over ACP".to_string(),
        }
    }

    /// Verifies the manifest serializes with camelCase keys and embedded entrypoint.
    #[test]
    fn serializes_plugin_manifest_contract() {
        assert_eq!(
            serde_json::to_value(&sample_manifest()).unwrap(),
            json!({
                "id": "codex",
                "version": "0.1.0",
                "kind": "agent",
                "entrypoint": {
                    "program": "node",
                    "args": ["plugins/codex/adapter.js"],
                    "envs": [["CODEX_PATH", "/usr/bin/codex"]]
                },
                "displayName": "Codex",
                "description": "Bridges Ora to the codex agent over ACP",
            })
        );
    }

    /// Verifies the managed plugin view carries state and source path.
    #[test]
    fn serializes_managed_plugin_contract() {
        let plugin = Plugin {
            id: "codex".to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::Agent,
            entrypoint: sample_entrypoint(),
            display_name: "Codex".to_string(),
            description: "Bridges Ora to the codex agent over ACP".to_string(),
            state: PluginState::Enabled,
            source_path: "plugins/codex".to_string(),
        };

        assert_eq!(
            serde_json::to_value(&plugin).unwrap(),
            json!({
                "id": "codex",
                "version": "0.1.0",
                "kind": "agent",
                "entrypoint": {
                    "program": "node",
                    "args": ["plugins/codex/adapter.js"],
                    "envs": [["CODEX_PATH", "/usr/bin/codex"]]
                },
                "displayName": "Codex",
                "description": "Bridges Ora to the codex agent over ACP",
                "state": "enabled",
                "sourcePath": "plugins/codex",
            })
        );
    }

    /// Verifies discovered plugin wraps the manifest with its source path.
    #[test]
    fn serializes_discovered_plugin_contract() {
        let discovered = DiscoveredPlugin {
            manifest: sample_manifest(),
            source_path: "plugins/codex".to_string(),
        };
        let value = serde_json::to_value(&discovered).unwrap();
        assert_eq!(value["sourcePath"], json!("plugins/codex"));
        assert_eq!(value["manifest"]["id"], json!("codex"));
    }

    /// Verifies the handshake DTOs round-trip with camelCase keys.
    #[test]
    fn serializes_initialize_handshake_contracts() {
        let request = InitializeRequest {
            protocol_version: "0.1.0".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            json!({ "protocolVersion": "0.1.0" })
        );

        let response = InitializeResponse {
            kind: PluginKind::Agent,
            version: "0.1.0".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&response).unwrap(),
            json!({ "kind": "agent", "version": "0.1.0" })
        );
    }
}
