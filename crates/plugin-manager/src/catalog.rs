//! Plugin catalog: the diagnostic view over managed plugins (design-v3 §5.5, §7.1).
//!
//! The catalog keeps *every* managed candidate — valid, invalid, incompatible, unsupported or
//! integrity-mismatched — so the UI can explain why a plugin has no runtime admission (§7.1). It
//! never silently drops a bad manifest. The orthogonal status enums come from §5.5: a directory's
//! manifest can be schema-valid yet runtime-incompatible, runtime-compatible yet unsupported for
//! execution, and all three can be fine yet its integrity can mismatch the receipt.

use std::path::PathBuf;

use ora_plugin_protocol::{PluginId, PluginKindTag, PluginManifest};
use serde::{Deserialize, Serialize};

use crate::enablement::EffectiveDisableReason;

/// A structured diagnostic surfaced to the UI/logs.
///
/// The design does not freeze a closed diagnostic code set for MVP; each diagnostic carries a
/// human-readable message plus the orthogonal status it pertains to where useful.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDiagnostic {
    pub message: String,
}

impl PluginDiagnostic {
    /// Constructs a diagnostic from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Why an engine declaration is incompatible (§5.5). The three independent axes from §5.1.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CompatibilityReason {
    /// `engines.ora` does not match the Ora app version.
    OraVersion,
    /// `engines.pluginApi` is not the exact required version.
    PluginApi,
    /// `engines.bun` does not match the pinned Bun range.
    BunVersion,
}

/// Whether the manifest file/schema itself is valid (§5.5), independent of runtime support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManifestValidity {
    Valid,
    Invalid { diagnostics: Vec<PluginDiagnostic> },
}

/// Whether the current Ora/pluginApi/Bun/OS support this manifest (§5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeCompatibility {
    Compatible,
    Incompatible { reason: CompatibilityReason },
}

/// Whether the Host implements this kind/contribution's executor (§5.5).
///
/// A valid, compatible `workbench` is `UnsupportedKind` in the MVP — it is managed (install/list/
/// disable/uninstall) but never executed (§5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeSupport {
    Supported,
    UnsupportedSchemaVersion { manifest_version: u32 },
    UnsupportedKind { kind: PluginKindTag },
}

/// Whether the managed copy matches its receipt digest (§5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntegrityStatus {
    /// The managed copy matches the receipt tree digest.
    Matches,
    /// The managed copy does not match the receipt digest.
    Mismatch,
    /// No receipt or no managed files to compare.
    Missing,
}

/// One catalog row: a managed candidate plus its orthogonal statuses (§7.1).
///
/// `plugin_id` is `None` when the manifest could not even yield a canonical id; such a row stays in
/// the catalog for diagnosis but has no runtime admission.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub plugin_id: Option<PluginId>,
    pub location: PathBuf,
    pub manifest: Option<PluginManifest>,
    pub validity: ManifestValidity,
    pub compatibility: RuntimeCompatibility,
    pub support: RuntimeSupport,
    pub integrity: IntegrityStatus,
    pub diagnostics: Vec<PluginDiagnostic>,
}

impl CatalogEntry {
    /// Returns the disable reasons implied by this entry's orthogonal statuses (§7.2).
    ///
    /// Integrity/pending-removal/missing-files are surfaced by the installer/state layer; this only
    /// derives the manifest/engine/support reasons so [`crate::enablement::EffectiveEnablement`] can
    /// pick the primary reason by the strict total order.
    pub fn disable_reasons(&self) -> Vec<EffectiveDisableReason> {
        let mut reasons = Vec::new();
        if !matches!(self.validity, ManifestValidity::Valid) {
            reasons.push(EffectiveDisableReason::InvalidManifest);
        }
        if matches!(
            self.compatibility,
            RuntimeCompatibility::Incompatible { .. }
        ) {
            reasons.push(EffectiveDisableReason::IncompatibleEngine);
        }
        // Both unsupported-runtime axes (§5.5) fail closed to `UnsupportedKind`: an unknown
        // `manifestVersion` (`UnsupportedSchemaVersion`) must NOT enable/spawn, just like an
        // unsupported kind. §5.5 forbids routing an unknown schema version through the validity
        // axis, so the support→enablement derivation must fail closed here independently of the
        // enable-command guard (§17.5.5).
        if matches!(
            self.support,
            RuntimeSupport::UnsupportedKind { .. }
                | RuntimeSupport::UnsupportedSchemaVersion { .. }
        ) {
            reasons.push(EffectiveDisableReason::UnsupportedKind);
        }
        if matches!(self.integrity, IntegrityStatus::Mismatch) {
            reasons.push(EffectiveDisableReason::IntegrityMismatch);
        }
        if matches!(self.integrity, IntegrityStatus::Missing) {
            reasons.push(EffectiveDisableReason::MissingInstallFiles);
        }
        reasons
    }
}

/// A snapshot of the full catalog (§7.1, §15.1 `scan_installed`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PluginCatalogSnapshot {
    pub entries: Vec<CatalogEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn runtime_support_unsupported_kind_projects_camelcase() {
        let support = RuntimeSupport::UnsupportedKind {
            kind: PluginKindTag::Workbench,
        };
        assert_eq!(
            serde_json::to_value(&support).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "unsupportedKind": { "kind": "workbench" } })
        );
        assert_eq!(
            serde_json::to_value(RuntimeSupport::Supported)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("supported")
        );
    }

    #[test]
    fn integrity_status_serializes_camelcase() {
        assert_eq!(
            serde_json::to_value(IntegrityStatus::Matches)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("matches")
        );
        assert_eq!(
            serde_json::to_value(IntegrityStatus::Mismatch)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("mismatch")
        );
        assert_eq!(
            serde_json::to_value(IntegrityStatus::Missing)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("missing")
        );
    }

    #[test]
    fn catalog_entry_disable_reasons_follow_orthogonal_statuses() {
        let entry = CatalogEntry {
            plugin_id: None,
            location: PathBuf::from("/x"),
            manifest: None,
            validity: ManifestValidity::Invalid {
                diagnostics: vec![PluginDiagnostic::new("bad id")],
            },
            compatibility: RuntimeCompatibility::Incompatible {
                reason: CompatibilityReason::BunVersion,
            },
            support: RuntimeSupport::UnsupportedKind {
                kind: PluginKindTag::Workbench,
            },
            integrity: IntegrityStatus::Mismatch,
            diagnostics: vec![],
        };
        let reasons = entry.disable_reasons();
        assert!(reasons.contains(&EffectiveDisableReason::InvalidManifest));
        assert!(reasons.contains(&EffectiveDisableReason::IncompatibleEngine));
        assert!(reasons.contains(&EffectiveDisableReason::UnsupportedKind));
        assert!(reasons.contains(&EffectiveDisableReason::IntegrityMismatch));
        // Primary must be the highest-priority: IntegrityMismatch (7) > InvalidManifest (6) >
        // IncompatibleEngine (5) > UnsupportedKind (4).
        assert_eq!(
            crate::enablement::primary_reason(&reasons),
            Some(EffectiveDisableReason::IntegrityMismatch)
        );

        // A clean entry has no disable reasons.
        let clean = CatalogEntry {
            plugin_id: None,
            location: PathBuf::from("/x"),
            manifest: None,
            validity: ManifestValidity::Valid,
            compatibility: RuntimeCompatibility::Compatible,
            support: RuntimeSupport::Supported,
            integrity: IntegrityStatus::Matches,
            diagnostics: vec![],
        };
        assert!(clean.disable_reasons().is_empty());
    }

    #[test]
    fn unsupported_schema_version_fail_closes_to_unsupported_kind() {
        // §5.5: an unknown manifestVersion (UnsupportedSchemaVersion) must not enable/spawn even
        // when validity/compatibility/integrity are otherwise fine; it maps to UnsupportedKind so the
        // support→enablement derivation fails closed.
        let entry = CatalogEntry {
            plugin_id: None,
            location: PathBuf::from("/x"),
            manifest: None,
            validity: ManifestValidity::Valid,
            compatibility: RuntimeCompatibility::Compatible,
            support: RuntimeSupport::UnsupportedSchemaVersion { manifest_version: 2 },
            integrity: IntegrityStatus::Matches,
            diagnostics: vec![],
        };
        let reasons = entry.disable_reasons();
        assert!(reasons.contains(&EffectiveDisableReason::UnsupportedKind));
        assert_eq!(
            crate::enablement::primary_reason(&reasons),
            Some(EffectiveDisableReason::UnsupportedKind)
        );
        // An unsupported-schema entry cannot derive Enabled.
        assert_eq!(
            crate::enablement::EffectiveEnablement::from(
                crate::enablement::UserEnablement::Enabled,
                &reasons,
            ),
            crate::enablement::EffectiveEnablement::Disabled(EffectiveDisableReason::UnsupportedKind)
        );
    }
}
