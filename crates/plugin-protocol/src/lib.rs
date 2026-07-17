mod agent;
mod frame;
mod identity;
mod json;
mod json_rpc;
mod lifecycle;
mod manifest;
mod serde_util;

pub use manifest::{
    AgentContribution, AgentContributions, MANIFEST_VERSION_V1, MAX_CONTRIBUTIONS,
    MAX_DISPLAY_NAME_UTF16_LEN, ManifestError, PluginEngines, PluginKindManifest, PluginKindTag,
    PluginManifest, PluginRelativePath, SemverRange, WorkbenchContributions,
};

pub use serde_util::strict_option;

pub use agent::{
    AGENT_BUSINESS_ERROR_CODE, AGENT_METHOD_CANCEL_CONVERSATION,
    AGENT_METHOD_DISCOVER_INSTALLATIONS, AGENT_METHOD_GET_CONFIGURATION_SUMMARY,
    AGENT_METHOD_LIST_CONVERSATIONS, AGENT_METHOD_LIST_MCP_SERVERS, AGENT_METHOD_LIST_SKILLS,
    AGENT_METHOD_SEND_MESSAGE, AGENT_METHOD_START_CONVERSATION, AgentAvailability,
    AgentBusinessFailureKind, AgentConfigurationItem, AgentConfigurationValue,
    AgentConversationSummary, AgentDiscoveryDiagnostic, AgentDiscoveryDiagnosticKind, AgentEvent,
    AgentFinishReason, AgentInstallation, AgentMcpServerSummary, AgentMcpTransport,
    AgentOutputChannel, AgentResourceSource, AgentScope, AgentSkillSummary, AgentTurnResult,
    AgentUsage, CancelConversationRequest, CancelConversationResponse, CancelDisposition,
    DiscoverInstallationsRequest, DiscoverInstallationsResponse, GetConfigurationSummaryRequest,
    GetConfigurationSummaryResponse, InvocationSemantics, ListConversationsRequest,
    ListConversationsResponse, ListMcpServersRequest, ListMcpServersResponse, ListSkillsRequest,
    ListSkillsResponse, SendMessageRequest, StartConversationRequest, invocation_semantics,
};

pub use json::{DEFAULT_MAX_DEPTH, StrictJsonError, parse_strict, parse_strict_object};

pub use lifecycle::{
    ActivateParams, ActivateReason, ActivateResult, ActivatedProvider, DeactivateParams,
    DeactivateReason, DeactivateResult, DeclaredAgent, InitializeLimits, InitializeParams,
    InitializePaths, InitializePlugin, InitializeResult, InitializeResultPlugin, LifecycleError,
    PluginVersion,
};

pub use json_rpc::{
    AgentBusinessErrorData, CancelRequestParams, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST,
    JSONRPC_VERSION, JsonObject, JsonRpcEnvelopeError, JsonRpcError, JsonRpcErrorResponse,
    JsonRpcNotification, JsonRpcRequest, JsonRpcSuccessResponse, JsonRpcVersion, JsonValue,
    METHOD_ACTIVATE, METHOD_CANCEL_REQUEST, METHOD_DEACTIVATE, METHOD_EXIT, METHOD_INITIALIZE,
    METHOD_NOT_FOUND, METHOD_STREAM, PARSE_ERROR, REQUEST_CANCELLED, REQUEST_ID_MAX_BYTES,
    RequestId, SERVER_BUSY, StreamNotificationParams, WIRE_VERSION,
};

pub use frame::{
    FrameDecoder, FrameError, FrameType, HEADER_LEN, MAX_PAYLOAD_BYTES, decode_frame, encode_frame,
    parse_header, parse_length,
};

pub use identity::{
    AGENT_CONFIG_KEY_MAX_BYTES, AGENT_PROMPT_MAX_BYTES, AGENT_PROVIDER_ID_MAX_BYTES,
    AgentConfigurationKey, AgentConversationId, AgentCursor, AgentInstallationId, AgentPageLimit,
    AgentPrompt, AgentProviderId, AgentResourceId, AgentToolCallId, AgentTurnId, ClientRequestId,
    ContentOwnerId, FiniteJsonNumber, HOST_RESOLVED_PATH_MAX_BYTES, HostResolvedAbsolutePath,
    IdentityError, JSON_SAFE_U64_MAX, JsonSafeU64, OPAQUE_ID_MAX_BYTES, PLUGIN_ID_MAX_BYTES,
    PluginId, ProjectHandle, RFC3339_MAX_BYTES, Rfc3339Timestamp, SessionId, WorktreeHandle,
};

use std::path::Path;
use ts_rs::{Config, ExportError, TS};

/// Exports plugin protocol DTOs for TypeScript SDK packages that speak the same wire format.
///
/// This is the single source of truth for the SDK type surface (§22.5); SDK packages must not
/// hand-write parallel data interfaces for these types.
pub fn export_typescript_bindings_to(
    output_directory: impl AsRef<Path>,
) -> Result<(), ExportError> {
    let config = Config::new().with_out_dir(output_directory.as_ref());

    // Identity leaf newtypes (transparent JSON primitives).
    AgentConfigurationKey::export(&config)?;
    AgentConversationId::export(&config)?;
    AgentCursor::export(&config)?;
    AgentInstallationId::export(&config)?;
    AgentPageLimit::export(&config)?;
    AgentPrompt::export(&config)?;
    AgentProviderId::export(&config)?;
    AgentResourceId::export(&config)?;
    AgentToolCallId::export(&config)?;
    AgentTurnId::export(&config)?;
    ClientRequestId::export(&config)?;
    ContentOwnerId::export(&config)?;
    FiniteJsonNumber::export(&config)?;
    HostResolvedAbsolutePath::export(&config)?;
    JsonSafeU64::export(&config)?;
    PluginId::export(&config)?;
    ProjectHandle::export(&config)?;
    Rfc3339Timestamp::export(&config)?;
    SessionId::export(&config)?;
    WorktreeHandle::export(&config)?;

    // Agent Contract v1 DTOs (§13.1).
    AgentScope::export(&config)?;
    AgentAvailability::export(&config)?;
    AgentConfigurationValue::export(&config)?;
    AgentResourceSource::export(&config)?;
    AgentEvent::export(&config)?;
    AgentOutputChannel::export(&config)?;
    AgentMcpTransport::export(&config)?;
    AgentFinishReason::export(&config)?;
    CancelDisposition::export(&config)?;
    AgentBusinessFailureKind::export(&config)?;
    AgentDiscoveryDiagnosticKind::export(&config)?;
    InvocationSemantics::export(&config)?;
    AgentUsage::export(&config)?;
    AgentTurnResult::export(&config)?;
    AgentConversationSummary::export(&config)?;
    AgentSkillSummary::export(&config)?;
    AgentMcpServerSummary::export(&config)?;
    AgentConfigurationItem::export(&config)?;
    AgentInstallation::export(&config)?;
    AgentDiscoveryDiagnostic::export(&config)?;
    DiscoverInstallationsResponse::export(&config)?;
    GetConfigurationSummaryResponse::export(&config)?;
    ListSkillsResponse::export(&config)?;
    ListMcpServersResponse::export(&config)?;
    ListConversationsResponse::export(&config)?;
    CancelConversationResponse::export(&config)?;
    DiscoverInstallationsRequest::export(&config)?;
    GetConfigurationSummaryRequest::export(&config)?;
    ListSkillsRequest::export(&config)?;
    ListMcpServersRequest::export(&config)?;
    ListConversationsRequest::export(&config)?;
    StartConversationRequest::export(&config)?;
    SendMessageRequest::export(&config)?;
    CancelConversationRequest::export(&config)?;

    // Manifest v1 (§5).
    SemverRange::export(&config)?;
    PluginRelativePath::export(&config)?;
    PluginEngines::export(&config)?;
    AgentContribution::export(&config)?;
    AgentContributions::export(&config)?;
    WorkbenchContributions::export(&config)?;
    PluginKindManifest::export(&config)?;
    PluginKindTag::export(&config)?;
    PluginManifest::export(&config)?;

    // Wire envelope + control message DTOs (§12.5, §12.6, §12.7, §16.1).
    JsonValue::export(&config)?;
    JsonObject::export(&config)?;
    JsonRpcVersion::export(&config)?;
    RequestId::export(&config)?;
    JsonRpcError::export(&config)?;
    JsonRpcRequest::export(&config)?;
    JsonRpcSuccessResponse::export(&config)?;
    JsonRpcErrorResponse::export(&config)?;
    JsonRpcNotification::export(&config)?;
    AgentBusinessErrorData::export(&config)?;
    StreamNotificationParams::export(&config)?;
    CancelRequestParams::export(&config)?;

    // Lifecycle handshake DTOs (§12.8).
    PluginVersion::export(&config)?;
    InitializeParams::export(&config)?;
    InitializePlugin::export(&config)?;
    InitializePaths::export(&config)?;
    DeclaredAgent::export(&config)?;
    InitializeLimits::export(&config)?;
    InitializeResult::export(&config)?;
    InitializeResultPlugin::export(&config)?;
    ActivateReason::export(&config)?;
    ActivateParams::export(&config)?;
    ActivatedProvider::export(&config)?;
    ActivateResult::export(&config)?;
    DeactivateReason::export(&config)?;
    DeactivateParams::export(&config)?;
    DeactivateResult::export(&config)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::export_typescript_bindings_to;
    use std::fs;
    use tempfile::TempDir;

    /// Verifies SDK protocol bindings are written only to the caller-selected package directory and
    /// that key v1 surfaces (tagged unions, transparent primitives) are present.
    #[test]
    fn exports_typescript_protocol_bindings() {
        let output_directory = TempDir::new().unwrap_or_else(|error| {
            panic!("failed to create protocol export directory: {error}");
        });

        export_typescript_bindings_to(output_directory.path()).unwrap_or_else(|error| {
            panic!("expected protocol export to succeed: {error}");
        });

        let generated_source =
            fs::read_to_string(output_directory.path().join("plugin-protocol.ts"))
                .unwrap_or_else(|error| panic!("failed to read protocol export: {error}"));

        // Transparent primitives.
        assert!(
            generated_source.contains("export type PluginId = string;"),
            "PluginId must export as a transparent string"
        );
        assert!(
            generated_source.contains("export type JsonSafeU64 = number;"),
            "JsonSafeU64 must export as a transparent number"
        );
        // Tagged unions (§13.1 `type` / `kind` discriminants).
        assert!(
            generated_source.contains("AgentScope"),
            "AgentScope must be exported"
        );
        assert!(
            generated_source.contains("AgentEvent"),
            "AgentEvent must be exported"
        );
        assert!(
            generated_source.contains("StartConversationRequest"),
            "StartConversationRequest must be exported"
        );

        let exported_type_count = generated_source
            .lines()
            .filter(|line| line.starts_with("export type "))
            .count();
        assert!(
            exported_type_count >= 40,
            "expected at least 40 exported types, got {exported_type_count}"
        );
    }
}
