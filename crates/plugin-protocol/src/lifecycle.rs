//! Lifecycle handshake DTOs (design-v3 §12.8).
//!
//! The Agent v1 lifecycle is fixed as `$/initialize` Request/Response → `$/activate`
//! Request/Response → Running; stop is `$/deactivate` Request/Response → `$/exit` Notification →
//! wait/kill. There is no `plugin.ready`/`$/ready` or double-ready state. These DTOs are the typed
//! `params`/`result` payloads of the control methods; the raw envelope is in [`crate::json_rpc`].
//!
//! Field names serialize to lowerCamelCase; every object recursively rejects unknown fields, and an
//! `Option` may be omitted but never carries an explicit `null` (via [`crate::serde_util::strict_option`]).

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use crate::identity::{
    AgentProviderId, ContentOwnerId, HostResolvedAbsolutePath, PluginId, SessionId,
};
use crate::manifest::PluginKindTag;

// ---------------------------------------------------------------------------
// Plugin version (top-level package.json SemVer, §5.1)
// ---------------------------------------------------------------------------

/// A strict SemVer plugin version (the top-level `package.json` `version`, §5.1).
///
/// Stored as the canonical text so it round-trips transparently; validated at construction via the
/// `semver` crate. The manifest `ora` object does not carry the version; it comes from the
/// package.json top level and is fed into `$/initialize` as `plugin.version`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct PluginVersion(String);

impl PluginVersion {
    /// Constructs a validated SemVer version.
    pub fn try_new(value: String) -> Result<Self, LifecycleError> {
        semver::Version::parse(&value).map_err(|error| LifecycleError::InvalidSemverVersion {
            version: value.clone(),
            reason: error.to_string(),
        })?;
        Ok(Self(value))
    }

    /// Returns the raw version text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Errors produced while constructing lifecycle DTOs.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LifecycleError {
    #[error("invalid SemVer version `{version}`: {reason}")]
    InvalidSemverVersion { version: String, reason: String },
}

// ---------------------------------------------------------------------------
// $/initialize (§12.8)
// ---------------------------------------------------------------------------

/// `$/initialize` request params (§12.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct InitializeParams {
    #[ts(type = "number")]
    pub wire_version: u32,
    pub host_version: String,
    pub runtime_version: String,
    pub session_id: SessionId,
    pub plugin: InitializePlugin,
    pub paths: InitializePaths,
    pub declared_agents: Vec<DeclaredAgent>,
    pub limits: InitializeLimits,
}

/// The plugin descriptor sent in `$/initialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct InitializePlugin {
    pub id: PluginId,
    pub version: PluginVersion,
    pub kind: PluginKindTag,
    #[ts(type = "number")]
    pub plugin_api: u32,
    pub content_owner: ContentOwnerId,
}

/// Host-derived managed paths sent in `$/initialize` (§12.8).
///
/// `entryPath` is for the private bootstrap to import during `$/activate`; it is not exposed to the
/// author `ExtensionContext`. None of these is selectable by the plugin or the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct InitializePaths {
    pub extension_path: HostResolvedAbsolutePath,
    pub entry_path: HostResolvedAbsolutePath,
    pub storage_path: HostResolvedAbsolutePath,
}

/// One agent contribution declared in the manifest, echoed in `$/initialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct DeclaredAgent {
    pub id: AgentProviderId,
    #[ts(type = "number")]
    pub contract_version: u32,
}

/// The seven exact `$/initialize` `limits` (§12.8, §13.1).
///
/// `maxFrameBytes` must equal the wire v1 constant; the other six are per-generation caps the Host
/// may tighten but not raise above the v1 hard caps. The plugin has no `limits` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct InitializeLimits {
    #[ts(type = "number")]
    pub max_frame_bytes: u32,
    #[ts(type = "number")]
    pub max_pending_requests: u32,
    #[ts(type = "number")]
    pub max_agent_event_bytes: u32,
    #[ts(type = "number")]
    pub max_agent_result_bytes: u32,
    #[ts(type = "number")]
    pub max_agent_prompt_bytes: u32,
    #[ts(type = "number")]
    pub max_active_turns: u32,
    #[ts(type = "number")]
    pub max_page_items: u32,
}

/// `$/initialize` response (§12.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct InitializeResult {
    #[ts(type = "number")]
    pub wire_version: u32,
    pub runtime_version: String,
    pub session_id: SessionId,
    pub plugin: InitializeResultPlugin,
}

/// The plugin identity echoed in `$/initialize` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct InitializeResultPlugin {
    pub id: PluginId,
    pub version: PluginVersion,
}

// ---------------------------------------------------------------------------
// $/activate (§12.8)
// ---------------------------------------------------------------------------

/// Why the Host is activating the plugin (§12.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum ActivateReason {
    /// Activation triggered by the first invocation (lazy start).
    LazyInvocation,
    /// Activation triggered by an explicit `start` command.
    ManualStart,
}

/// `$/activate` request params (§12.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ActivateParams {
    pub reason: ActivateReason,
}

/// One provider descriptor returned by `$/activate`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ActivatedProvider {
    pub id: AgentProviderId,
    #[ts(type = "number")]
    pub contract_version: u32,
}

/// `$/activate` response (§12.8).
///
/// `providers` must be deep-equal to the manifest `contributes.agents` (canonical id sort); extra,
/// missing, duplicate or version-mismatched providers are `ActivationFailed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ActivateResult {
    pub providers: Vec<ActivatedProvider>,
}

// ---------------------------------------------------------------------------
// $/deactivate + $/exit (§12.8)
// ---------------------------------------------------------------------------

/// Why the Host is deactivating the plugin (§12.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum DeactivateReason {
    ManualStop,
    Disable,
    Uninstall,
    Shutdown,
    GrantChanged,
}

/// `$/deactivate` request params (§12.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct DeactivateParams {
    pub reason: DeactivateReason,
}

/// `$/deactivate` response (§12.8: `result` is `null`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export_to = "plugin-protocol.ts", type = "null")]
pub struct DeactivateResult;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn session() -> SessionId {
        SessionId::try_new("sess-1".to_string()).unwrap_or_else(|e| panic!("session: {e}"))
    }

    #[test]
    fn plugin_version_validates_semver() {
        assert!(PluginVersion::try_new("0.1.0".to_string()).is_ok());
        assert!(PluginVersion::try_new("1.2.3-beta+exp".to_string()).is_ok());
        assert!(PluginVersion::try_new("not-a-version".to_string()).is_err());
    }

    #[test]
    fn initialize_params_projects_camelcase_and_round_trips() {
        let params = InitializeParams {
            wire_version: 1,
            host_version: "0.1.0".to_string(),
            runtime_version: "0.1.0".to_string(),
            session_id: session(),
            plugin: InitializePlugin {
                id: PluginId::try_new("ora.claude-code".to_string())
                    .unwrap_or_else(|e| panic!("id: {e}")),
                version: PluginVersion::try_new("0.1.0".to_string())
                    .unwrap_or_else(|e| panic!("version: {e}")),
                kind: PluginKindTag::Agent,
                plugin_api: 1,
                content_owner: ContentOwnerId::try_new(format!("sha256-{}", "a".repeat(64)))
                    .unwrap_or_else(|e| panic!("owner: {e}")),
            },
            paths: InitializePaths {
                extension_path: HostResolvedAbsolutePath::try_new(
                    r"D:\plugins\ora.claude-code".to_string(),
                )
                .unwrap_or_else(|e| panic!("path: {e}")),
                entry_path: HostResolvedAbsolutePath::try_new(
                    r"D:\plugins\ora.claude-code\dist\index.js".to_string(),
                )
                .unwrap_or_else(|e| panic!("path: {e}")),
                storage_path: HostResolvedAbsolutePath::try_new(
                    r"D:\plugin-data\ora.claude-code\sha256-owner".to_string(),
                )
                .unwrap_or_else(|e| panic!("path: {e}")),
            },
            declared_agents: vec![DeclaredAgent {
                id: AgentProviderId::try_new("claude-code".to_string())
                    .unwrap_or_else(|e| panic!("provider: {e}")),
                contract_version: 1,
            }],
            limits: InitializeLimits {
                max_frame_bytes: 8 * 1024 * 1024,
                max_pending_requests: 128,
                max_agent_event_bytes: 256 * 1024,
                max_agent_result_bytes: 1024 * 1024,
                max_agent_prompt_bytes: 1024 * 1024,
                max_active_turns: 64,
                max_page_items: 100,
            },
        };
        let value = serde_json::to_value(&params).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["wireVersion"], json!(1));
        assert_eq!(value["plugin"]["kind"], json!("agent"));
        assert_eq!(value["plugin"]["pluginApi"], json!(1));
        assert_eq!(
            value["paths"]["entryPath"],
            json!(r"D:\plugins\ora.claude-code\dist\index.js")
        );
        assert_eq!(value["declaredAgents"][0]["id"], json!("claude-code"));
        assert_eq!(value["limits"]["maxFrameBytes"], json!(8 * 1024 * 1024));
        // Round-trip + strict unknown-field rejection.
        let parsed: InitializeParams =
            serde_json::from_value(value).unwrap_or_else(|e| panic!("deserialize: {e}"));
        let mut with_extra =
            serde_json::to_value(&parsed).unwrap_or_else(|e| panic!("serialize: {e}"));
        if let serde_json::Value::Object(ref mut map) = with_extra {
            map.insert("rogue".to_string(), json!("nope"));
        }
        assert!(serde_json::from_value::<InitializeParams>(with_extra).is_err());
    }

    #[test]
    fn activate_and_deactivate_reasons_project_to_camelcase() {
        assert_eq!(
            serde_json::to_value(ActivateReason::LazyInvocation)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("lazyInvocation")
        );
        assert_eq!(
            serde_json::to_value(DeactivateReason::GrantChanged)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("grantChanged")
        );
        let result = ActivateResult {
            providers: vec![ActivatedProvider {
                id: AgentProviderId::try_new("claude-code".to_string())
                    .unwrap_or_else(|e| panic!("provider: {e}")),
                contract_version: 1,
            }],
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "providers": [{ "id": "claude-code", "contractVersion": 1 }] })
        );
    }

    #[test]
    fn deactivate_result_is_null() {
        assert_eq!(
            serde_json::to_value(DeactivateResult).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!(null)
        );
        let _: DeactivateResult =
            serde_json::from_value(json!(null)).unwrap_or_else(|e| panic!("deserialize null: {e}"));
    }
}
