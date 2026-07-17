//! Facade/supporting types (design-v3 §8.1, §14.3, §15.1).
//!
//! These are the small, design-given types the management facade (`PluginManagement`/`
//! AgentPluginRuntime`, §15.1) and the `PluginError` enum (§16.1) reference. The facade traits
//! themselves return the full `PluginError` and the install/identify result types, which are added
//! once `PluginError` and the launch-grant model are complete.

use ora_plugin_protocol::{ContentOwnerId, DeactivateReason, PluginId};
use serde::{Deserialize, Serialize};

/// Why mutable plugin data is removed (§15.1). Not a bool on `uninstall`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DataRemovalScope {
    /// Remove only the current content owner's mutable data.
    CurrentContentOwner,
    /// Remove mutable data for all content owners (requires destructive confirmation).
    AllOwners,
}

/// A grant binding key (§14.3, §16.1): a launch grant is keyed by at least
/// `plugin_id + content_owner + grant_schema_version`, so a revoked grant cannot be silently
/// reused after reinstall.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantBindingKey {
    pub plugin_id: PluginId,
    pub content_owner: ContentOwnerId,
    pub schema_version: u32,
}

/// A Host-configured candidate discovery root id (§8.1). `scan_candidates(root_ids)` references
/// only these opaque ids; the Host maps each to a configured path — the client never submits a path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscoveryRootId(String);

impl DiscoveryRootId {
    /// Constructs a discovery root id (Host-configured, trusted).
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque, session-bound, single-`identify`-consumable selection bearer (§8.1, §15.3).
///
/// Issued by `scan_candidates`/the native picker; the client cannot construct or parse it. Bindings
/// (canonical source path, volume/file identity, TTL, audit id) live in the Host's
/// `CandidateAuthority`, not in this bearer.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SelectionHandle(String);

impl SelectionHandle {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque, single-`install`-consumable candidate bearer (§8.1, §15.3).
///
/// Minted by `identify`; binds the reviewed id/version/tree-digest + session + TTL + audit id.
/// `install` consumes it once and re-verifies root identity + staging digest (§9.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CandidateHandle(String);

impl CandidateHandle {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The reason the runtime is stopping a plugin (§10.2–§10.4, §11.6, §15.1).
///
/// The Host maps a stop directly to the `$/deactivate` reason it sends on the wire (§12.8), so this
/// is the runtime-side view of [`DeactivateReason`].
pub type StopReason = DeactivateReason;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn pid() -> PluginId {
        PluginId::try_new("ora.claude-code".to_string()).unwrap_or_else(|e| panic!("pid: {e}"))
    }

    fn owner() -> ContentOwnerId {
        ContentOwnerId::try_new(format!("sha256-{}", "a".repeat(64)))
            .unwrap_or_else(|e| panic!("owner: {e}"))
    }

    #[test]
    fn data_removal_scope_projects_camelcase() {
        assert_eq!(
            serde_json::to_value(DataRemovalScope::CurrentContentOwner)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("currentContentOwner")
        );
        assert_eq!(
            serde_json::to_value(DataRemovalScope::AllOwners)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("allOwners")
        );
    }

    #[test]
    fn grant_binding_key_round_trips() {
        let key = GrantBindingKey {
            plugin_id: pid(),
            content_owner: owner(),
            schema_version: 1,
        };
        let value = serde_json::to_value(&key).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["pluginId"], json!("ora.claude-code"));
        assert_eq!(value["schemaVersion"], json!(1));
        let parsed: GrantBindingKey =
            serde_json::from_value(value).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert_eq!(parsed, key);
    }

    #[test]
    fn opaque_handles_serialize_as_transparent_strings() {
        assert_eq!(
            serde_json::to_value(DiscoveryRootId::new("home")).unwrap_or_else(|e| panic!("s: {e}")),
            json!("home")
        );
        assert_eq!(
            serde_json::to_value(SelectionHandle::new("sel-abc"))
                .unwrap_or_else(|e| panic!("s: {e}")),
            json!("sel-abc")
        );
        assert_eq!(
            serde_json::to_value(CandidateHandle::new("cand-xyz"))
                .unwrap_or_else(|e| panic!("s: {e}")),
            json!("cand-xyz")
        );
    }

    #[test]
    fn stop_reason_is_the_deactivate_reason_set() {
        assert_eq!(
            serde_json::to_value(StopReason::Disable).unwrap_or_else(|e| panic!("s: {e}")),
            json!("disable")
        );
        assert_eq!(
            serde_json::to_value(StopReason::GrantChanged).unwrap_or_else(|e| panic!("s: {e}")),
            json!("grantChanged")
        );
    }
}
