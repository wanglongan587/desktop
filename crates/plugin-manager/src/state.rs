//! Persistent plugin-system state model (design-v3 §6.4).
//!
//! `plugin-system/state.json` saves only Host state (never runtime state: PID/Starting/Running/
//! pending-RPC are not persisted). `revision` is monotonic per successful mutation; `pendingOperations`
//! is a discriminant union (not a shapeless array); `crashPolicy` is a discriminant union too.
//! Missing/corrupt state fails closed to `Disabled` (§6.4, §3.7).
//!
//! This module is the typed model; the single-writer state-store actor (atomic temp/replace,
//! backup recovery, quarantine) is a separate, I/O-gated piece.

use std::path::PathBuf;

use ora_plugin_protocol::{ContentOwnerId, JsonSafeU64, PluginId, PluginVersion};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::digest::TreeDigest;
use crate::enablement::UserEnablement;

use std::fmt;

/// A host-issued, opaque operation id (§6.4). Does not participate in plugin identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperationId(String);

impl OperationId {
    /// Constructs an operation id, validating it is non-empty and ≤256 UTF-8 bytes with no NUL.
    pub fn try_new(value: String) -> Result<Self, StateModelError> {
        validate_opaque_id(&value, "operation id")?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A host-issued, opaque candidate-audit id (§15.1): the audit fact of a user
/// selection/discovery→identify→consume chain; not a client-supplied string.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CandidateAuditId(String);

impl CandidateAuditId {
    /// Constructs a candidate-audit id with the same opaque-id rules.
    pub fn try_new(value: String) -> Result<Self, StateModelError> {
        validate_opaque_id(&value, "candidate audit id")?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CandidateAuditId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Maximum bytes for an opaque state id (§13.1 opaque rules: 1..=256).
const OPAQUE_STATE_ID_MAX_BYTES: usize = 256;

/// Validates an opaque state id: 1..=256 UTF-8 bytes, no NUL/C0/C1 control.
fn validate_opaque_id(value: &str, label: &str) -> Result<(), StateModelError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(StateModelError::EmptyId {
            label: label.to_string(),
        });
    }
    if bytes.len() > OPAQUE_STATE_ID_MAX_BYTES {
        return Err(StateModelError::IdTooLong {
            label: label.to_string(),
            len: bytes.len(),
            max: OPAQUE_STATE_ID_MAX_BYTES,
        });
    }
    for ch in value.chars() {
        let code = u32::from(ch);
        if code == 0 || code <= 0x1F || (0x7F..=0x9F).contains(&code) {
            return Err(StateModelError::IdInvalidCharacter {
                label: label.to_string(),
            });
        }
    }
    Ok(())
}

/// Errors produced while constructing state-model values.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StateModelError {
    #[error("{label} must be non-empty")]
    EmptyId { label: String },
    #[error("{label} exceeds {max} bytes (got {len})")]
    IdTooLong {
        label: String,
        len: usize,
        max: usize,
    },
    #[error("{label} must not contain NUL or C0/C1 control characters")]
    IdInvalidCharacter { label: String },
    #[error("content digest must be `sha256:<64 lowercase hex>` (got {0})")]
    InvalidContentDigest(String),
}

/// A content digest in its display form `sha256:<64 lowercase hex>` (§6.2 receipt, §6.4 state).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentDigest(String);

impl ContentDigest {
    /// Constructs a content digest from a `sha256:<64 hex>` string.
    pub fn try_new(value: String) -> Result<Self, StateModelError> {
        validate_content_digest(&value)?;
        Ok(Self(value))
    }

    /// Constructs the persisted content digest from a computed [`TreeDigest`].
    pub fn from_tree_digest(digest: &TreeDigest) -> Self {
        Self(digest.as_display())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validates a `sha256:<64 lowercase hex>` content digest.
fn validate_content_digest(value: &str) -> Result<(), StateModelError> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(StateModelError::InvalidContentDigest(value.to_string()));
    };
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StateModelError::InvalidContentDigest(value.to_string()));
    }
    Ok(())
}

/// A managed `.trash` location for a pending removal (§6.1, §6.4).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManagedTrashLocation(PathBuf);

impl ManagedTrashLocation {
    pub fn new(path: PathBuf) -> Self {
        Self(path)
    }
    pub fn as_path(&self) -> &PathBuf {
        &self.0
    }
}

/// The phase of a pending install (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PendingInstallPhase {
    /// Staging prepared + receipt written; final rename not yet committed.
    Prepared,
    /// Staging renamed to final; state commit pending.
    FilesCommitted,
}

/// The phase of a pending removal (§6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PendingRemovalPhase {
    /// Tombstone written, admission closed; final→trash rename pending.
    Prepared,
    /// Final renamed to trash; install-record removal + async delete pending.
    FilesMoved,
}

/// A pending install journal entry (§6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingInstall {
    pub operation_id: OperationId,
    pub plugin_id: PluginId,
    pub expected_version: PluginVersion,
    pub expected_digest: ContentDigest,
    pub candidate_audit_id: CandidateAuditId,
    pub phase: PendingInstallPhase,
}

/// A pending removal journal entry (§6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PendingRemoval {
    pub operation_id: OperationId,
    pub plugin_id: PluginId,
    pub expected_digest: ContentDigest,
    pub install_operation_id: OperationId,
    pub trash_location: ManagedTrashLocation,
    pub phase: PendingRemovalPhase,
}

/// The `pendingOperations` discriminant union (§6.4: not a shapeless array).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PendingOperation {
    Install(PendingInstall),
    Remove(PendingRemoval),
}

/// The persisted installation record (§6.4). `plugin_id` comes from the map key, not a field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Installation {
    /// Only `Installed` is persisted; `UntrackedInstall`/`MissingInstallFiles` are reconcile-computed.
    pub state: InstallationState,
    pub plugin_version: PluginVersion,
    pub content_digest: ContentDigest,
    pub content_owner: ContentOwnerId,
    pub install_operation_id: OperationId,
}

/// Persisted installation state (§6.4). MVP persists only `Installed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallationState {
    Installed,
}

/// The crash-policy discriminant union (§6.4): normal or blocked by a crash loop.
///
/// `recent_crashes_unix_ms` has a fixed window cap and is trimmed on every state mutation; entering
/// `BlockedByCrashLoop` persists across restarts until an explicit `reset_crash_loop`/disable→enable
/// (§6.4, §11.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "state",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CrashPolicy {
    Normal {
        recent_crashes_unix_ms: Vec<JsonSafeU64>,
    },
    BlockedByCrashLoop {
        recent_crashes_unix_ms: Vec<JsonSafeU64>,
        blocked_at_unix_ms: JsonSafeU64,
    },
}

/// One per-plugin state entry (§6.4 `plugins[id]`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginStateEntry {
    pub user_enablement: UserEnablement,
    pub installation: Installation,
    pub crash_policy: CrashPolicy,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn pending_install_operation_projects_typed_envelope() {
        let op = PendingOperation::Install(PendingInstall {
            operation_id: OperationId::try_new("op-1".to_string())
                .unwrap_or_else(|e| panic!("op: {e}")),
            plugin_id: PluginId::try_new("ora.claude-code".to_string())
                .unwrap_or_else(|e| panic!("pid: {e}")),
            expected_version: PluginVersion::try_new("0.1.0".to_string())
                .unwrap_or_else(|e| panic!("ver: {e}")),
            expected_digest: ContentDigest::try_new(format!("sha256:{}", "a".repeat(64)))
                .unwrap_or_else(|e| panic!("digest: {e}")),
            candidate_audit_id: CandidateAuditId::try_new("audit-1".to_string())
                .unwrap_or_else(|e| panic!("audit: {e}")),
            phase: PendingInstallPhase::Prepared,
        });
        let value = serde_json::to_value(&op).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["type"], json!("install"));
        assert_eq!(value["operationId"], json!("op-1"));
        assert_eq!(value["phase"], json!("prepared"));
        assert_eq!(value["expectedVersion"], json!("0.1.0"));
        // Round-trip preserves the discriminant.
        let parsed: PendingOperation =
            serde_json::from_value(value).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert!(matches!(parsed, PendingOperation::Install(_)));
    }

    #[test]
    fn pending_removal_round_trips_and_tags_remove() {
        let op = PendingOperation::Remove(PendingRemoval {
            operation_id: OperationId::try_new("op-2".to_string())
                .unwrap_or_else(|e| panic!("op: {e}")),
            plugin_id: PluginId::try_new("ora.x".to_string())
                .unwrap_or_else(|e| panic!("pid: {e}")),
            expected_digest: ContentDigest::try_new(format!("sha256:{}", "b".repeat(64)))
                .unwrap_or_else(|e| panic!("digest: {e}")),
            install_operation_id: OperationId::try_new("op-1".to_string())
                .unwrap_or_else(|e| panic!("op: {e}")),
            trash_location: ManagedTrashLocation::new(PathBuf::from("/trash/op-2")),
            phase: PendingRemovalPhase::FilesMoved,
        });
        let value = serde_json::to_value(&op).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["type"], json!("remove"));
        assert_eq!(value["phase"], json!("filesMoved"));
        assert_eq!(value["trashLocation"], json!("/trash/op-2"));
        let parsed: PendingOperation =
            serde_json::from_value(value).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert!(matches!(parsed, PendingOperation::Remove(_)));
    }

    #[test]
    fn crash_policy_discriminant_projects_state_tag() {
        let normal = CrashPolicy::Normal {
            recent_crashes_unix_ms: vec![],
        };
        assert_eq!(
            serde_json::to_value(&normal).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "state": "normal", "recentCrashesUnixMs": [] })
        );
        let blocked = CrashPolicy::BlockedByCrashLoop {
            recent_crashes_unix_ms: vec![
                JsonSafeU64::try_new(1000).unwrap_or_else(|e| panic!("n: {e}")),
            ],
            blocked_at_unix_ms: JsonSafeU64::try_new(2000).unwrap_or_else(|e| panic!("n: {e}")),
        };
        let value = serde_json::to_value(&blocked).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["state"], json!("blockedByCrashLoop"));
        assert_eq!(value["blockedAtUnixMs"], json!(2000));
    }

    #[test]
    fn content_digest_from_tree_digest_is_sha256_hex() {
        let digest = crate::digest::compute_tree_digest(&[]);
        let cd = ContentDigest::from_tree_digest(&digest);
        assert!(cd.as_str().starts_with("sha256:"));
        assert!(ContentDigest::try_new(cd.as_str().to_string()).is_ok());
        assert!(ContentDigest::try_new("not-a-digest".to_string()).is_err());
        assert!(ContentDigest::try_new("sha256:abc".to_string()).is_err());
    }

    #[test]
    fn opaque_ids_reject_empty_long_and_control() {
        assert!(OperationId::try_new("op-1".to_string()).is_ok());
        assert!(OperationId::try_new("".to_string()).is_err());
        assert!(OperationId::try_new("a\tb".to_string()).is_err());
        assert!(OperationId::try_new("a".repeat(257)).is_err());
    }
}
