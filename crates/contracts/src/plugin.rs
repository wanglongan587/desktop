use ora_plugin_protocol::{
    AgentEvent, AgentMethod, AgentProviderKey, ContentDigest, ContentOwnerId, JsonSafeU64,
    PluginId, PluginPackageManifest, PluginVersion,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use ts_rs::{Config, ExportError, TS};

pub const PLUGINS_PATH: &str = "/api/plugins";
pub const PLUGIN_PATH: &str = "/api/plugins/{id}";
pub const PLUGIN_SCAN_PATH: &str = "/api/plugins/scan";
pub const PLUGIN_IDENTIFY_PATH: &str = "/api/plugins/identify";
pub const PLUGIN_INSTALL_PATH: &str = "/api/plugins/install";
pub const PLUGIN_ENABLE_PATH: &str = "/api/plugins/{id}/enable";
pub const PLUGIN_DISABLE_PATH: &str = "/api/plugins/{id}/disable";
pub const PLUGIN_LAUNCH_GRANT_PATH: &str = "/api/plugins/{id}/launch-grant";
pub const PLUGIN_RESET_CRASH_LOOP_PATH: &str = "/api/plugins/{id}/reset-crash-loop";
pub const PLUGIN_REMOVE_DATA_PATH: &str = "/api/plugins/{id}/remove-data";
pub const PLUGIN_START_PATH: &str = "/api/plugins/{id}/start";
pub const PLUGIN_STOP_PATH: &str = "/api/plugins/{id}/stop";
pub const AGENT_INVOCATIONS_PATH: &str = "/api/agent-invocations";
pub const AGENT_INVOCATION_PATH: &str = "/api/agent-invocations/{id}";
pub const INVOCATION_ID_HEADER: &str = "x-ora-invocation-id";

/// Describes one agent contribution from an installed plugin package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct InstalledPluginAgent {
    pub id: String,
    pub display_name: String,
    pub contract_version: u32,
}

/// Describes one installed plugin discovered from its package manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct InstalledPlugin {
    pub id: String,
    pub package_name: String,
    pub display_name: String,
    pub version: String,
    pub kind: String,
    pub main: String,
    pub agents: Vec<InstalledPluginAgent>,
}

/// Requests the immutable startup snapshot of installed plugin packages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct ListInstalledPluginsRequest {}

/// Returns every valid installed plugin in stable identifier order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin.ts")]
pub struct ListInstalledPluginsResponse {
    pub plugins: Vec<InstalledPlugin>,
}

/// Exports the mainline installed-plugin snapshot contracts through ts-rs.
pub(crate) fn export(config: &Config) -> Result<(), ExportError> {
    InstalledPluginAgent::export(config)?;
    InstalledPlugin::export(config)?;
    ListInstalledPluginsRequest::export(config)?;
    ListInstalledPluginsResponse::export(config)?;
    Ok(())
}

/// Requests inert candidate discovery through configured Host root identifiers only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanPluginsRequest {
    pub root_ids: Vec<String>,
}

/// Consumes one opaque, session-bound selection authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentifyPluginRequest {
    pub selection_handle: String,
}

/// Consumes one opaque, digest-bound installation authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallPluginRequest {
    pub candidate_handle: String,
}

/// Selects mutable data without encoding a destructive operation as a boolean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "scope",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum RemovePluginDataRequest {
    CurrentContentOwner {},
    AllOwners { confirmation_handle: String },
}

/// Names an application object that the Host resolves into an AgentScope and canonical path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ApplicationAgentScope {
    Global {},
    Project {
        project_id: String,
    },
    Worktree {
        project_id: String,
        worktree_id: String,
    },
}

/// Carries a closed Agent method while keeping filesystem paths under Host authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentInvocationRequest {
    pub plugin_id: PluginId,
    pub method: AgentMethod,
    pub scope: ApplicationAgentScope,
    #[ts(type = "unknown")]
    pub params: Value,
}

/// One safe management diagnostic without an installed filesystem location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDiagnosticView {
    pub code: String,
    pub message: String,
}

/// One installed catalog row projected without its authoritative local path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCatalogItem {
    pub plugin_id: Option<PluginId>,
    pub manifest: Option<PluginPackageManifest>,
    pub validity: String,
    pub compatibility: String,
    pub support: String,
    pub integrity: String,
    pub diagnostics: Vec<PluginDiagnosticView>,
}

/// Returns the immutable installed catalog projection at one revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCatalogResponse {
    pub revision: JsonSafeU64,
    pub plugins: Vec<PluginCatalogItem>,
}

/// A display-only candidate paired with opaque identify authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSelectionView {
    pub selection_handle: String,
    pub display_name: String,
}

/// Returns inert candidates and never returns their authoritative local paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScanPluginsResponse {
    pub candidates: Vec<CandidateSelectionView>,
}

/// Returns reviewed package facts and the new single-use installation authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentifyPluginResponse {
    pub plugin_id: PluginId,
    pub plugin_version: PluginVersion,
    pub content_digest: ContentDigest,
    pub candidate_handle: String,
    pub manifest: PluginPackageManifest,
    pub compatibility: String,
    pub support: String,
    pub diagnostics: Vec<PluginDiagnosticView>,
}

/// Returns the committed install identity; installation is always disabled initially.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstallPluginResponse {
    pub plugin_id: PluginId,
    pub plugin_version: PluginVersion,
    pub content_digest: ContentDigest,
    pub content_owner: ContentOwnerId,
    pub enabled: bool,
}

/// One unresolved Host-owned launch value reference; no resolved value is serializable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PluginLaunchValueReference {
    HostConfiguration { key: String },
    Credential { key: String },
    DiscoveredExecutable { provider: AgentProviderKey },
    AuthorizedPath { path_id: String },
}

/// Maps one approved Host reference into a named child environment variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginEnvironmentBinding {
    pub target: String,
    pub value: PluginLaunchValueReference,
}

/// Stores grant metadata for the plugin selected by the route, without duplicating its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetPluginLaunchGrantRequest {
    pub content_owner: ContentOwnerId,
    pub schema_version: u32,
    pub revision: JsonSafeU64,
    pub environment: Vec<PluginEnvironmentBinding>,
}

/// Safe persisted launch-grant metadata returned without resolving any values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginLaunchGrantView {
    pub plugin_id: PluginId,
    pub content_owner: ContentOwnerId,
    pub schema_version: u32,
    pub revision: JsonSafeU64,
    pub environment: Vec<PluginEnvironmentBinding>,
}

/// Returns the current grant metadata, if one is configured for this content owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetPluginLaunchGrantResponse {
    pub grant: Option<PluginLaunchGrantView>,
}

/// A successful command with no additional application payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginActionResponse {}

/// One compact line in the authenticated application NDJSON stream.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum AgentInvocationStreamEnvelope {
    Event {
        event: AgentEvent,
    },
    Completed {
        #[ts(type = "unknown")]
        result: Value,
    },
    Failed {
        error: String,
    },
}

/// Value returned only by the trusted native-picker IPC command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativePluginSelectionResponse {
    pub selection: Option<CandidateSelectionView>,
}

/// Value returned only after a trusted destructive-confirmation dialog succeeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DataRemovalConfirmationResponse {
    pub confirmation_handle: String,
}

/// Writes the plugin application contract as one module with explicit protocol imports.
pub(crate) fn export_typescript_bindings_to(output_directory: &Path) -> Result<(), ExportError> {
    let config = Config::new();
    let mut source = String::from(
        "// Generated from ora-contracts. Do not edit.\n\n\
         import type { AgentEvent, AgentMethod, AgentProviderKey, ContentDigest, ContentOwnerId, JsonSafeU64, PluginId, PluginPackageManifest, PluginVersion } from \"./plugin-protocol.js\";\n\n",
    );
    macro_rules! push_declarations {
        ($($type:ty),+ $(,)?) => {
            $(
                source.push_str("export ");
                source.push_str(&<$type>::decl(&config));
                source.push_str("\n\n");
            )+
        };
    }
    push_declarations!(
        ScanPluginsRequest,
        IdentifyPluginRequest,
        InstallPluginRequest,
        RemovePluginDataRequest,
        ApplicationAgentScope,
        AgentInvocationRequest,
        PluginDiagnosticView,
        PluginCatalogItem,
        PluginCatalogResponse,
        CandidateSelectionView,
        ScanPluginsResponse,
        IdentifyPluginResponse,
        InstallPluginResponse,
        PluginLaunchValueReference,
        PluginEnvironmentBinding,
        SetPluginLaunchGrantRequest,
        PluginLaunchGrantView,
        GetPluginLaunchGrantResponse,
        PluginActionResponse,
        AgentInvocationStreamEnvelope,
        NativePluginSelectionResponse,
        DataRemovalConfirmationResponse,
    );
    let normalized_length = source.trim_end().len();
    source.truncate(normalized_length);
    source.push('\n');
    std::fs::write(output_directory.join("plugin-contracts.ts"), source)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ApplicationAgentScope, RemovePluginDataRequest};
    use serde_json::json;

    /// Proves path-bearing or ambiguous scope fields are rejected at the application boundary.
    #[test]
    fn application_scope_rejects_client_working_directory() {
        assert!(
            serde_json::from_value::<ApplicationAgentScope>(json!({
                "type": "project",
                "projectId": "project-1",
                "workingDirectory": "C:\\attacker"
            }))
            .is_err()
        );
    }

    /// Proves deleting all historical owners requires a separate one-time capability.
    #[test]
    fn all_owner_data_removal_requires_confirmation_handle() {
        assert!(
            serde_json::from_value::<RemovePluginDataRequest>(json!({ "scope": "allOwners" }))
                .is_err()
        );
    }
}
