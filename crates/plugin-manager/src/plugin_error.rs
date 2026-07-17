//! The management-layer error type (design-v3 §16.1).
//!
//! `PluginError` is what the `PluginManagement`/`AgentPluginRuntime` facades (§15.1) return. It
//! aggregates the stable runtime-failure classifications (`failure.rs`), the orthogonal status
//! reasons (`catalog.rs`/`enablement.rs`), the host-state ids (`state.rs`) and the opaque
//! handles/keys (`facade.rs`). Variants with under-specified sub-enums (`ProtocolFailure`/
//! `HandshakeFailure`/`ActivationFailure`/`DeactivationFailure`) derive their closed variant sets
//! from the wire/lifecycle/ABI rules in §12.5/§12.8/§13.2; they carry no attacker-controlled free
//! text (sensitive detail goes into bounded `PluginDiagnostic`/tracing).

use ora_plugin_protocol::{PluginId, PluginKindTag, PluginVersion};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog::{CompatibilityReason, PluginDiagnostic};
use crate::enablement::EffectiveDisableReason;
use crate::facade::GrantBindingKey;
use crate::failure::{AgentContractFailure, TransportFailureStage, UnknownOutcomeCause};
use crate::state::{ContentDigest, OperationId};

/// The manifest kind, for `UnsupportedKind` (§16.1). Alias of the `agent`/`workbench` tag.
pub type PluginKind = PluginKindTag;

// ---------------------------------------------------------------------------
// Two-phase-authorization handle failures (§16.1: at least these distinctions).
// ---------------------------------------------------------------------------

/// Why a `SelectionHandle` was rejected (§15.1, §16.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum SelectionHandleFailure {
    #[error("selection handle is unknown")]
    Unknown,
    #[error("selection handle expired")]
    Expired,
    #[error("selection handle belongs to a different session")]
    WrongSession,
    #[error("selection handle was presented for the wrong purpose")]
    WrongPurpose,
    #[error("selection handle was already consumed")]
    AlreadyConsumed,
}

/// Why a `CandidateHandle` was rejected (§15.1, §16.1).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum CandidateHandleFailure {
    #[error("candidate handle is unknown")]
    Unknown,
    #[error("candidate handle expired")]
    Expired,
    #[error("candidate handle belongs to a different session")]
    WrongSession,
    #[error("candidate handle was presented for the wrong purpose")]
    WrongPurpose,
    #[error("candidate handle was already consumed")]
    AlreadyConsumed,
}

// ---------------------------------------------------------------------------
// Source-authorization change reasons (§16.1 verbatim).
// ---------------------------------------------------------------------------

/// An opaque, pinned source-root identity (volume/file identity, §8.1).
///
/// Concrete Win32 `BY_HANDLE_FILE_INFORMATION` is the scanner/safe_fs layer's concern; this is the
/// comparable, opaque identity used in `SourceChangeReason`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceRootIdentity(String);

impl SourceRootIdentity {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The reviewed identity frozen at `identify` time (§8.1, §16.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewedPluginIdentity {
    pub plugin_id: PluginId,
    pub version: PluginVersion,
    pub digest: ContentDigest,
}

/// Why the source no longer matches the reviewed candidate (§16.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SourceChangeReason {
    #[error("source root is missing")]
    RootMissing,
    #[error("source root identity mismatch")]
    RootIdentityMismatch {
        expected: SourceRootIdentity,
        actual: SourceRootIdentity,
    },
    #[error("staging validation failed")]
    StagingValidationFailed { diagnostics: Vec<PluginDiagnostic> },
    #[error("plugin identity mismatch")]
    PluginIdentityMismatch {
        expected: ReviewedPluginIdentity,
        actual: ReviewedPluginIdentity,
    },
    #[error("content digest mismatch")]
    ContentDigestMismatch {
        expected: ContentDigest,
        actual: ContentDigest,
    },
}

// ---------------------------------------------------------------------------
// Runtime sub-failures derived from §12.5/§12.8/§13.2.
// ---------------------------------------------------------------------------

/// Wire-protocol failures (§12.5). Derived from the frame/JSON/envelope rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum ProtocolFailure {
    #[error("frame length is zero/negative or exceeds the cap")]
    InvalidLength,
    #[error("unknown frame type")]
    UnknownFrameType,
    #[error("partial frame EOF")]
    PartialEof,
    #[error("invalid UTF-8 or JSON")]
    JsonParse,
    #[error("duplicate object key")]
    DuplicateKey,
    #[error("JSON nesting depth exceeded")]
    DepthExceeded,
    #[error("envelope shape does not match its frame type")]
    EnvelopeShapeMismatch,
    #[error("frame type does not match the envelope")]
    TypeEnvelopeMismatch,
    #[error("direction violation (e.g. plugin→host request)")]
    DirectionViolation,
}

/// `$/initialize`/`$/activate` handshake failures (§12.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum HandshakeFailure {
    #[error("wire version mismatch")]
    WireVersionMismatch,
    #[error("bootstrap response was not the first child frame")]
    ResponseNotFirst,
    #[error("session id mismatch")]
    SessionIdMismatch,
    #[error("$/initialize shape invalid")]
    InitializeShapeInvalid,
    #[error("$/activate shape invalid")]
    ActivateShapeInvalid,
    #[error("activate provider descriptor mismatch")]
    ProviderDescriptorMismatch,
    #[error("unexpected traffic before handshake completed")]
    UnexpectedTraffic,
}

/// `$/activate` provider-load failures (§13.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum ActivationFailure {
    #[error("default export is not a plain structural definition")]
    DefaultExportInvalid,
    #[error("provider id mismatch")]
    ProviderIdMismatch,
    #[error("provider contract version mismatch")]
    ProviderContractMismatch,
    #[error("provider is missing a required method")]
    MissingProviderMethod,
    #[error("activate threw")]
    ActivateThrew,
    #[error("activate returned an extra provider")]
    ExtraProvider,
}

/// `$/deactivate`/disposal failures (§13.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum DeactivationFailure {
    #[error("deactivate threw")]
    DeactivateThrew,
    #[error("disposal exceeded its deadline")]
    DisposalTimeout,
}

// ---------------------------------------------------------------------------
// The management-layer error enum (§16.1 verbatim).
// ---------------------------------------------------------------------------

/// The error the plugin-management facades return (§16.1).
#[derive(Debug, Clone, PartialEq, Error, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PluginError {
    #[error("plugin not found: {plugin_id}")]
    NotFound { plugin_id: PluginId },
    #[error("plugin already installed: {plugin_id}")]
    AlreadyInstalled { plugin_id: PluginId },
    #[error("manifest is invalid")]
    InvalidManifest { diagnostics: Vec<PluginDiagnostic> },
    #[error("unsupported manifest schema version {manifest_version}")]
    UnsupportedSchemaVersion { manifest_version: u32 },
    #[error("unsupported package layout")]
    UnsupportedPackageLayout { diagnostics: Vec<PluginDiagnostic> },
    #[error("incompatible: {reason:?}")]
    Incompatible { reason: CompatibilityReason },
    #[error("unsupported kind: {kind:?}")]
    UnsupportedKind { kind: PluginKind },
    #[error("integrity mismatch for {plugin_id}")]
    IntegrityMismatch { plugin_id: PluginId },
    #[error("missing install files for {plugin_id}")]
    MissingInstallFiles { plugin_id: PluginId },
    #[error("plugin {plugin_id} is disabled: {reason:?}")]
    Disabled {
        plugin_id: PluginId,
        reason: EffectiveDisableReason,
    },
    #[error("install conflict for {plugin_id}")]
    InstallConflict { plugin_id: PluginId },
    #[error("selection handle invalid: {reason}")]
    SelectionHandleInvalid { reason: SelectionHandleFailure },
    #[error("candidate handle invalid: {reason}")]
    CandidateHandleInvalid { reason: CandidateHandleFailure },
    #[error("source changed: {reason}")]
    SourceChanged { reason: SourceChangeReason },
    #[error("recovery required for operation {operation_id}")]
    RecoveryRequired {
        operation_id: OperationId,
        diagnostic: PluginDiagnostic,
    },
    #[error("removal pending for {plugin_id}")]
    RemovalPending { plugin_id: PluginId },
    #[error("state corrupt: {message}")]
    StateCorrupt { message: String },
    #[error("unsupported state schema version {schema_version}")]
    StateVersionUnsupported { schema_version: u32 },
    #[error("persistence uncertain for operation {operation_id}")]
    PersistenceUncertain {
        operation_id: OperationId,
        diagnostic: PluginDiagnostic,
    },
    #[error("data directory is in use")]
    DataDirInUse,
    #[error("plugin runtime unavailable")]
    PluginRuntimeUnavailable { diagnostic: PluginDiagnostic },
    #[error("launch grant unavailable for {plugin_id}")]
    LaunchGrantUnavailable {
        plugin_id: PluginId,
        binding: GrantBindingKey,
    },
    #[error("process spawn failed for {plugin_id}: {message}")]
    ProcessSpawnFailed {
        plugin_id: PluginId,
        message: String,
    },
    #[error("handshake failed for {plugin_id}: {reason}")]
    HandshakeFailed {
        plugin_id: PluginId,
        reason: HandshakeFailure,
    },
    #[error("activation failed for {plugin_id}: {reason}")]
    ActivationFailed {
        plugin_id: PluginId,
        reason: ActivationFailure,
    },
    #[error("deactivation failed for {plugin_id}: {reason}")]
    DeactivationFailed {
        plugin_id: PluginId,
        reason: DeactivationFailure,
    },
    #[error("protocol violation for {plugin_id}: {reason}")]
    ProtocolViolation {
        plugin_id: PluginId,
        reason: ProtocolFailure,
    },
    #[error("process tree kill unavailable for {plugin_id}")]
    TreeKillUnavailable {
        plugin_id: PluginId,
        diagnostic: PluginDiagnostic,
    },
    #[error("process tree cleanup timeout for {plugin_id} generation {generation}")]
    TreeCleanupTimeout {
        plugin_id: PluginId,
        generation: u64,
    },
    #[error("backpressure exceeded for {plugin_id} request {request_id}")]
    BackpressureExceeded {
        plugin_id: PluginId,
        request_id: String,
    },
    #[error("agent contract violation for {plugin_id} request {request_id}: {reason}")]
    AgentContractViolation {
        plugin_id: PluginId,
        request_id: String,
        reason: AgentContractFailure,
    },
    #[error("transport failed for {plugin_id} request {request_id}: {stage:?}")]
    TransportFailed {
        plugin_id: PluginId,
        request_id: String,
        stage: TransportFailureStage,
    },
    #[error("request timed out for {plugin_id} request {request_id}")]
    RequestTimedOut {
        plugin_id: PluginId,
        request_id: String,
    },
    #[error("request cancelled for {plugin_id} request {request_id}")]
    Cancelled {
        plugin_id: PluginId,
        request_id: String,
    },
    #[error("plugin {plugin_id} exited (code {exit_code:?})")]
    PluginExited {
        plugin_id: PluginId,
        exit_code: Option<i32>,
    },
    #[error("unknown outcome for {plugin_id} request {request_id}: {cause:?}")]
    UnknownOutcome {
        plugin_id: PluginId,
        request_id: String,
        cause: UnknownOutcomeCause,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn pid() -> PluginId {
        PluginId::try_new("ora.claude-code".to_string()).unwrap_or_else(|e| panic!("pid: {e}"))
    }

    #[test]
    fn not_found_and_disabled_project_camelcase() {
        assert_eq!(
            serde_json::to_value(PluginError::NotFound { plugin_id: pid() })
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "notFound": { "pluginId": "ora.claude-code" } })
        );
        assert_eq!(
            serde_json::to_value(PluginError::Disabled {
                plugin_id: pid(),
                reason: EffectiveDisableReason::CrashLoop
            })
            .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "disabled": { "pluginId": "ora.claude-code", "reason": "crashLoop" } })
        );
    }

    #[test]
    fn runtime_sub_enums_serialize_camelcase() {
        assert_eq!(
            serde_json::to_value(ProtocolFailure::PartialEof).unwrap_or_else(|e| panic!("s: {e}")),
            json!("partialEof")
        );
        assert_eq!(
            serde_json::to_value(HandshakeFailure::SessionIdMismatch)
                .unwrap_or_else(|e| panic!("s: {e}")),
            json!("sessionIdMismatch")
        );
        assert_eq!(
            serde_json::to_value(ActivationFailure::DefaultExportInvalid)
                .unwrap_or_else(|e| panic!("s: {e}")),
            json!("defaultExportInvalid")
        );
        assert_eq!(
            serde_json::to_value(DeactivationFailure::DisposalTimeout)
                .unwrap_or_else(|e| panic!("s: {e}")),
            json!("disposalTimeout")
        );
    }

    #[test]
    fn source_change_reason_round_trips() {
        let reason = SourceChangeReason::RootIdentityMismatch {
            expected: SourceRootIdentity::new("vol-a"),
            actual: SourceRootIdentity::new("vol-b"),
        };
        let value = serde_json::to_value(&reason).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["rootIdentityMismatch"]["expected"], json!("vol-a"));
        assert_eq!(value["rootIdentityMismatch"]["actual"], json!("vol-b"));
        let parsed: SourceChangeReason =
            serde_json::from_value(value).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert_eq!(parsed, reason);
    }

    #[test]
    fn selection_and_candidate_handle_failures_round_trip() {
        assert_eq!(
            serde_json::to_value(SelectionHandleFailure::AlreadyConsumed)
                .unwrap_or_else(|e| panic!("s: {e}")),
            json!("alreadyConsumed")
        );
        assert_eq!(
            serde_json::to_value(CandidateHandleFailure::WrongSession)
                .unwrap_or_else(|e| panic!("s: {e}")),
            json!("wrongSession")
        );
    }
}
