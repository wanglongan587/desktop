//! Agent Contract v1 DTOs (design-v3 §13.1).
//!
//! Every type here is the single source of truth for the wire/SDK surface: Rust defines the schema,
//! `ts-rs` generates the TypeScript DTOs, and the golden fixtures exercise Rust encode → TS decode
//! → TS encode → Rust decode. Field names serialize to lowerCamelCase; every object recursively
//! rejects unknown fields, and `Option<T>` may be omitted but never carries an explicit `null`.
//!
//! Tagged unions use the `type` (or `kind`) discriminant exactly as §13.1 specifies:
//! `AgentScope`, `AgentAvailability`, `AgentConfigurationValue`, `AgentResourceSource` use
//! `type`; `AgentEvent` uses `kind`. Plain enums (`AgentMcpTransport`, `AgentOutputChannel`,
//! `AgentFinishReason`, `CancelDisposition`, `AgentDiscoveryDiagnosticKind`) serialize as
//! lowerCamelCase JSON strings.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::identity::{
    AgentConfigurationKey, AgentConversationId, AgentCursor, AgentInstallationId, AgentPageLimit,
    AgentPrompt, AgentProviderId, AgentResourceId, AgentToolCallId, AgentTurnId, ClientRequestId,
    FiniteJsonNumber, HostResolvedAbsolutePath, JsonSafeU64, ProjectHandle, Rfc3339Timestamp,
    WorktreeHandle,
};
use crate::serde_util::strict_option;

// ---------------------------------------------------------------------------
// Method registry (§13.1). Host→Plugin Agent business methods use the `agent.*` namespace.
// ---------------------------------------------------------------------------

/// `agent.discoverInstallations` — enumerate locally installed agent installations.
pub const AGENT_METHOD_DISCOVER_INSTALLATIONS: &str = "agent.discoverInstallations";
/// `agent.getConfigurationSummary` — surface safe, redacted configuration items.
pub const AGENT_METHOD_GET_CONFIGURATION_SUMMARY: &str = "agent.getConfigurationSummary";
/// `agent.listSkills` — page over safe skill summaries.
pub const AGENT_METHOD_LIST_SKILLS: &str = "agent.listSkills";
/// `agent.listMcpServers` — page over safe MCP server summaries.
pub const AGENT_METHOD_LIST_MCP_SERVERS: &str = "agent.listMcpServers";
/// `agent.listConversations` — page over conversation summaries.
pub const AGENT_METHOD_LIST_CONVERSATIONS: &str = "agent.listConversations";
/// `agent.startConversation` — create a conversation and send the first prompt (streaming).
pub const AGENT_METHOD_START_CONVERSATION: &str = "agent.startConversation";
/// `agent.sendMessage` — send a follow-up prompt to an existing conversation (streaming).
pub const AGENT_METHOD_SEND_MESSAGE: &str = "agent.sendMessage";
/// `agent.cancelConversation` — stop an active turn (safety-control business method).
pub const AGENT_METHOD_CANCEL_CONVERSATION: &str = "agent.cancelConversation";

/// Invocation semantics for an Agent method (§12.6). Host never auto-replays either kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export_to = "plugin-protocol.ts")]
pub enum InvocationSemantics {
    /// Safe to retry without side effects; on transport loss returns `Cancelled`/`RequestTimedOut`.
    Idempotent,
    /// Never automatically replayed; on transport loss without a terminal returns `UnknownOutcome`.
    NonIdempotent,
}

/// Declares whether a method is idempotent (§13.1 v1 invocation semantics freeze).
pub fn invocation_semantics(method: &str) -> Option<InvocationSemantics> {
    match method {
        AGENT_METHOD_DISCOVER_INSTALLATIONS
        | AGENT_METHOD_GET_CONFIGURATION_SUMMARY
        | AGENT_METHOD_LIST_SKILLS
        | AGENT_METHOD_LIST_MCP_SERVERS
        | AGENT_METHOD_LIST_CONVERSATIONS
        | AGENT_METHOD_CANCEL_CONVERSATION => Some(InvocationSemantics::Idempotent),
        AGENT_METHOD_START_CONVERSATION | AGENT_METHOD_SEND_MESSAGE => {
            Some(InvocationSemantics::NonIdempotent)
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// AgentScope (§13.1): call context and audit fact, never a filesystem sandbox.
// ---------------------------------------------------------------------------

/// Call context for one Agent request, carrying Host-issued opaque handles and a resolved cwd.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentScope {
    Global,
    Project {
        project_handle: ProjectHandle,
        working_directory: HostResolvedAbsolutePath,
    },
    Worktree {
        project_handle: ProjectHandle,
        worktree_handle: WorktreeHandle,
        working_directory: HostResolvedAbsolutePath,
    },
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// Parameters for `agent.discoverInstallations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct DiscoverInstallationsRequest {
    pub provider_id: AgentProviderId,
    pub scope: AgentScope,
}

/// Parameters for `agent.getConfigurationSummary`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct GetConfigurationSummaryRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
}

/// Parameters for `agent.listSkills`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ListSkillsRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub cursor: Option<AgentCursor>,
    pub limit: AgentPageLimit,
}

/// Parameters for `agent.listMcpServers`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ListMcpServersRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub cursor: Option<AgentCursor>,
    pub limit: AgentPageLimit,
}

/// Parameters for `agent.listConversations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ListConversationsRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub cursor: Option<AgentCursor>,
    pub limit: AgentPageLimit,
}

/// Parameters for `agent.startConversation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct StartConversationRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub scope: AgentScope,
    pub client_request_id: ClientRequestId,
    pub prompt: AgentPrompt,
}

/// Parameters for `agent.sendMessage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct SendMessageRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub conversation_id: AgentConversationId,
    pub scope: AgentScope,
    pub client_request_id: ClientRequestId,
    pub prompt: AgentPrompt,
}

/// Parameters for `agent.cancelConversation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct CancelConversationRequest {
    pub provider_id: AgentProviderId,
    pub installation_id: AgentInstallationId,
    pub conversation_id: AgentConversationId,
    pub scope: AgentScope,
}

// ---------------------------------------------------------------------------
// Responses
// ---------------------------------------------------------------------------

/// Result of `agent.discoverInstallations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct DiscoverInstallationsResponse {
    pub installations: Vec<AgentInstallation>,
    pub diagnostics: Vec<AgentDiscoveryDiagnostic>,
}

/// One discovered agent installation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentInstallation {
    pub installation_id: AgentInstallationId,
    pub display_name: String,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub version: Option<String>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub location_display: Option<String>,
    pub availability: AgentAvailability,
}

/// Availability of one installation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentAvailability {
    Available,
    Unavailable { reason: String },
}

/// A discovery diagnostic emitted alongside installations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentDiscoveryDiagnostic {
    pub kind: AgentDiscoveryDiagnosticKind,
    pub message: String,
}

/// Stable classification of a discovery diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentDiscoveryDiagnosticKind {
    NotFound,
    PermissionDenied,
    ProbeFailed,
}

/// Result of `agent.getConfigurationSummary`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct GetConfigurationSummaryResponse {
    pub items: Vec<AgentConfigurationItem>,
}

/// One configuration item. Secrets can only be represented as `redacted` (no raw value variant).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentConfigurationItem {
    pub key: AgentConfigurationKey,
    pub display_name: String,
    pub source: AgentResourceSource,
    pub value: AgentConfigurationValue,
}

/// The (possibly redacted/unset) value of one configuration item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentConfigurationValue {
    Unset,
    Redacted,
    Boolean { value: bool },
    Number { value: FiniteJsonNumber },
    String { value: String },
    StringList { value: Vec<String> },
}

/// Origin of a resource surfaced by the provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentResourceSource {
    User,
    Project,
    Worktree,
    BuiltIn,
    Unknown {
        #[serde(default, deserialize_with = "strict_option")]
        #[ts(optional)]
        display: Option<String>,
    },
}

/// Result of `agent.listSkills`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ListSkillsResponse {
    pub items: Vec<AgentSkillSummary>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub next_cursor: Option<AgentCursor>,
}

/// A safe skill summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentSkillSummary {
    pub id: AgentResourceId,
    pub display_name: String,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub description: Option<String>,
    pub source: AgentResourceSource,
}

/// Result of `agent.listMcpServers`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ListMcpServersResponse {
    pub items: Vec<AgentMcpServerSummary>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub next_cursor: Option<AgentCursor>,
}

/// A safe MCP server summary (no command/env/token).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentMcpServerSummary {
    pub id: AgentResourceId,
    pub display_name: String,
    pub transport: AgentMcpTransport,
    pub enabled: bool,
    pub source: AgentResourceSource,
}

/// MCP transport kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentMcpTransport {
    Stdio,
    Http,
    Sse,
    Unknown,
}

/// Result of `agent.listConversations`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct ListConversationsResponse {
    pub items: Vec<AgentConversationSummary>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub next_cursor: Option<AgentCursor>,
}

/// A conversation summary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentConversationSummary {
    pub conversation_id: AgentConversationId,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub updated_at: Option<Rfc3339Timestamp>,
}

// ---------------------------------------------------------------------------
// Streaming events and terminal result (§13.1 AgentEvent / AgentTurnResult)
// ---------------------------------------------------------------------------

/// One streaming event from `agent.startConversation` / `agent.sendMessage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentEvent {
    ConversationStarted {
        conversation_id: AgentConversationId,
    },
    TextDelta {
        channel: AgentOutputChannel,
        text: String,
    },
    Status {
        phase: String,
        #[serde(default, deserialize_with = "strict_option")]
        #[ts(optional)]
        message: Option<String>,
    },
    ToolCall {
        call_id: AgentToolCallId,
        name: String,
        #[serde(default, deserialize_with = "strict_option")]
        #[ts(optional)]
        summary: Option<String>,
    },
    ToolResult {
        call_id: AgentToolCallId,
        is_error: bool,
        #[serde(default, deserialize_with = "strict_option")]
        #[ts(optional)]
        summary: Option<String>,
    },
    Usage {
        usage: AgentUsage,
    },
}

/// Output channel for a text delta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentOutputChannel {
    Assistant,
    Reasoning,
    Tool,
}

/// Token/cost usage for one turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentUsage {
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub input_tokens: Option<JsonSafeU64>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub output_tokens: Option<JsonSafeU64>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub cost_micros: Option<JsonSafeU64>,
}

/// Terminal result of a turn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentTurnResult {
    pub conversation_id: AgentConversationId,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub turn_id: Option<AgentTurnId>,
    pub finish_reason: AgentFinishReason,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub usage: Option<AgentUsage>,
}

/// Why a turn ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentFinishReason {
    Completed,
    Cancelled,
    Limit,
}

/// Result of `agent.cancelConversation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct CancelConversationResponse {
    pub disposition: CancelDisposition,
}

/// Outcome of cancelling an active turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum CancelDisposition {
    Accepted,
    AlreadyStopped,
}

// ---------------------------------------------------------------------------
// Agent business failure kinds (§16.1): -32000 + data.kind closed enum.
// ---------------------------------------------------------------------------

/// Closed set of agent business failure kinds carried in `-32000` error `data.kind`.
///
/// `ProviderFailure` is bootstrap-reserved: plugin authors cannot create it; the bootstrap
/// synthesizes it for raw throws/rejects/generator failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum AgentBusinessFailureKind {
    AgentUnavailable,
    AuthenticationRequired,
    InvalidAgentConfiguration,
    InstallationNotFound,
    ConversationNotFound,
    UnsupportedAgentCapability,
    InvalidState,
    PermissionDenied,
    CursorExpired,
    AgentProcessFailed,
    ProviderFailure,
}

/// Wire JSON-RPC error code reserved for agent business failures (`-32000`, §16.1).
pub const AGENT_BUSINESS_ERROR_CODE: i32 = -32000;

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::{Value, json};

    fn make_scope_project() -> AgentScope {
        AgentScope::Project {
            project_handle: ProjectHandle::try_new("p-1".to_string())
                .unwrap_or_else(|error| panic!("project handle: {error}")),
            working_directory: HostResolvedAbsolutePath::try_new(r"D:\projects\rustun".to_string())
                .unwrap_or_else(|error| panic!("path: {error}")),
        }
    }

    #[test]
    fn agent_scope_project_projects_to_type_tagged_envelope() {
        let scope = make_scope_project();
        let value =
            serde_json::to_value(&scope).unwrap_or_else(|error| panic!("serialize: {error}"));
        assert_eq!(
            value,
            json!({
                "type": "project",
                "projectHandle": "p-1",
                "workingDirectory": r"D:\projects\rustun",
            })
        );
    }

    #[test]
    fn agent_configuration_value_number_and_redacted_projections() {
        let number = AgentConfigurationValue::Number {
            value: FiniteJsonNumber::try_new(1.5)
                .unwrap_or_else(|error| panic!("finite number: {error}")),
        };
        assert_eq!(
            serde_json::to_value(&number).unwrap_or_else(|error| panic!("serialize: {error}")),
            json!({ "type": "number", "value": 1.5 })
        );

        let redacted = AgentConfigurationValue::Redacted;
        assert_eq!(
            serde_json::to_value(&redacted).unwrap_or_else(|error| panic!("serialize: {error}")),
            json!({ "type": "redacted" })
        );
    }

    #[test]
    fn agent_event_text_delta_projects_with_kind_tag() {
        let event = AgentEvent::TextDelta {
            channel: AgentOutputChannel::Assistant,
            text: "你好".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&event).unwrap_or_else(|error| panic!("serialize: {error}")),
            json!({
                "kind": "textDelta",
                "channel": "assistant",
                "text": "你好",
            })
        );
    }

    #[test]
    fn cancel_disposition_projects_to_camelcase_strings() {
        assert_eq!(
            serde_json::to_value(CancelDisposition::Accepted)
                .unwrap_or_else(|error| panic!("serialize: {error}")),
            json!("accepted")
        );
        assert_eq!(
            serde_json::to_value(CancelDisposition::AlreadyStopped)
                .unwrap_or_else(|error| panic!("serialize: {error}")),
            json!("alreadyStopped")
        );
    }

    #[test]
    fn start_conversation_request_round_trips_strictly() {
        let request = StartConversationRequest {
            provider_id: AgentProviderId::try_new("claude-code".to_string())
                .unwrap_or_else(|error| panic!("provider: {error}")),
            installation_id: AgentInstallationId::try_new("inst-1".to_string())
                .unwrap_or_else(|error| panic!("installation: {error}")),
            scope: make_scope_project(),
            client_request_id: ClientRequestId::try_new(
                "123e4567-e89b-12d3-a456-426614174000".to_string(),
            )
            .unwrap_or_else(|error| panic!("client request id: {error}")),
            prompt: AgentPrompt::try_new("hello".to_string())
                .unwrap_or_else(|error| panic!("prompt: {error}")),
        };
        let value =
            serde_json::to_value(&request).unwrap_or_else(|error| panic!("serialize: {error}"));
        let parsed: StartConversationRequest = serde_json::from_value(value.clone())
            .unwrap_or_else(|error| panic!("deserialize: {error}"));
        assert_eq!(
            serde_json::to_value(&parsed).unwrap_or_else(|error| panic!("serialize: {error}")),
            value
        );

        // Unknown field must be rejected.
        let mut with_extra = value;
        if let Value::Object(ref mut map) = with_extra {
            map.insert("rogueField".to_string(), json!("nope"));
        }
        assert!(serde_json::from_value::<StartConversationRequest>(with_extra).is_err());
    }

    #[test]
    fn invocation_semantics_freeze_matches_design() {
        assert_eq!(
            invocation_semantics(AGENT_METHOD_DISCOVER_INSTALLATIONS),
            Some(InvocationSemantics::Idempotent)
        );
        assert_eq!(
            invocation_semantics(AGENT_METHOD_START_CONVERSATION),
            Some(InvocationSemantics::NonIdempotent)
        );
        assert_eq!(
            invocation_semantics(AGENT_METHOD_CANCEL_CONVERSATION),
            Some(InvocationSemantics::Idempotent)
        );
        assert_eq!(invocation_semantics("agent.unknown"), None);
    }
}
