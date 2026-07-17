//! Facade result types (design-v3 §8.1, §15.1).
//!
//! These are the `scan_candidates`/`identify`/`install` return shapes the `PluginManagement`
//! facade (§15.1) produces. They carry only safe display fields plus opaque handles; the client
//! never receives a managed path, id, version or digest it could submit as authorization.

use ora_plugin_protocol::{ContentOwnerId, PluginId, PluginVersion};
use serde::{Deserialize, Serialize};

use crate::catalog::{CatalogEntry, PluginDiagnostic};
use crate::facade::{CandidateHandle, SelectionHandle};
use crate::state::ContentDigest;

/// One `scan_candidates` result: safe display fields + an opaque selection handle (§8.1, §15.3).
///
/// The embedded [`CatalogEntry`] carries the diagnostic view (validity/compatibility/support/
/// integrity + diagnostics); the [`SelectionHandle`] is the single-`identify`-consumable bearer.
/// Serialize-only: it is a facade output, and [`CatalogEntry`] embeds [`PluginManifest`] which is
/// parsed manually (not `Deserialize`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSelection {
    pub entry: CatalogEntry,
    pub selection_handle: SelectionHandle,
}

/// The `identify` result (§8.1): the reviewed identity + diagnostics + an install-consumable
/// candidate handle. The client reviews this, then `install` consumes the handle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentifiedPlugin {
    pub plugin_id: PluginId,
    pub version: PluginVersion,
    pub digest: ContentDigest,
    pub diagnostics: Vec<PluginDiagnostic>,
    pub candidate_handle: CandidateHandle,
}

/// The `install_authorized_candidate` result (§9.1, §15.1): install committed as installed +
/// disabled (§3.6); the caller must `enable` separately. Carries the install facts, not a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledPlugin {
    pub plugin_id: PluginId,
    pub version: PluginVersion,
    pub content_owner: ContentOwnerId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn pid() -> PluginId {
        PluginId::try_new("ora.claude-code".to_string()).unwrap_or_else(|e| panic!("pid: {e}"))
    }

    fn entry() -> CatalogEntry {
        use crate::catalog::{
            IntegrityStatus, ManifestValidity, RuntimeCompatibility, RuntimeSupport,
        };
        CatalogEntry {
            plugin_id: Some(pid()),
            location: std::path::PathBuf::from("/src/ora.claude-code"),
            manifest: None,
            validity: ManifestValidity::Valid,
            compatibility: RuntimeCompatibility::Compatible,
            support: RuntimeSupport::Supported,
            integrity: IntegrityStatus::Matches,
            diagnostics: vec![],
        }
    }

    #[test]
    fn candidate_selection_carries_entry_and_handle() {
        let selection = CandidateSelection {
            entry: entry(),
            selection_handle: SelectionHandle::new("sel-1"),
        };
        let value = serde_json::to_value(&selection).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["selectionHandle"], json!("sel-1"));
        assert_eq!(value["entry"]["pluginId"], json!("ora.claude-code"));
    }

    #[test]
    fn identified_plugin_round_trips() {
        let identified = IdentifiedPlugin {
            plugin_id: pid(),
            version: PluginVersion::try_new("0.1.0".to_string())
                .unwrap_or_else(|e| panic!("ver: {e}")),
            digest: crate::state::ContentDigest::try_new(format!("sha256:{}", "a".repeat(64)))
                .unwrap_or_else(|e| panic!("digest: {e}")),
            diagnostics: vec![PluginDiagnostic::new("ok")],
            candidate_handle: CandidateHandle::new("cand-1"),
        };
        let value = serde_json::to_value(&identified).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["pluginId"], json!("ora.claude-code"));
        assert_eq!(value["candidateHandle"], json!("cand-1"));
        assert_eq!(value["diagnostics"][0]["message"], json!("ok"));
        let parsed: IdentifiedPlugin =
            serde_json::from_value(value).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert_eq!(parsed, identified);
    }

    #[test]
    fn installed_plugin_projects_camelcase() {
        let installed = InstalledPlugin {
            plugin_id: pid(),
            version: PluginVersion::try_new("0.1.0".to_string())
                .unwrap_or_else(|e| panic!("ver: {e}")),
            content_owner: ora_plugin_protocol::ContentOwnerId::try_new(format!(
                "sha256-{}",
                "a".repeat(64)
            ))
            .unwrap_or_else(|e| panic!("owner: {e}")),
        };
        let value = serde_json::to_value(&installed).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["pluginId"], json!("ora.claude-code"));
        assert_eq!(
            value["contentOwner"],
            json!(format!("sha256-{}", "a".repeat(64)))
        );
    }
}
