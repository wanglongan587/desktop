//! Runtime/transport failure classifications (design-v3 §16.1).
//!
//! These closed enums express *stable* failure classifications shared by the runtime actor
//! (§11.6) and the management-layer `PluginError` (§16.1). They carry no attacker-controlled free
//! text; sensitive detail goes into bounded, redacted `PluginDiagnostic`/tracing. The first fatal
//! trigger is write-once per bystander (§11.6); these types only describe the classification.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Where a transport write/read failed (§16.1, §11.6 writer-failure mapping).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum TransportFailureStage {
    /// A business Request frame write failed.
    #[error("request write failed")]
    RequestWrite,
    /// A `$/cancelRequest` write failed.
    #[error("transport cancel write failed")]
    TransportCancelWrite,
    /// Reading a response/stream frame failed (EOF/reader I/O/protocol fatal).
    #[error("response read failed")]
    ResponseRead,
    /// `$/exit`/session-control write or the drain after a fatal trigger failed.
    #[error("session drain failed")]
    SessionDrain,
}

/// The first fatal trigger's settled cause for bystanders without their own termination intent
/// (§11.6, §12.6). Write-once per bystander.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FatalSettlementCause {
    /// Connection lost at a transport stage (EOF/reader/writer fatal).
    #[error("connection lost at {stage}")]
    ConnectionLost { stage: TransportFailureStage },
    /// The process exited (with a code when known).
    #[error("process exited (code {exit_code:?})")]
    ProcessExited {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
    },
}

/// Why an agent DTO/event/result/business-error was rejected (§16.1). Terminal for the generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum AgentContractFailure {
    #[error("invalid request DTO")]
    InvalidRequestDto,
    #[error("invalid stream event")]
    InvalidStreamEvent,
    #[error("invalid terminal result")]
    InvalidTerminalResult,
    #[error("invalid business error")]
    InvalidBusinessError,
    #[error("conversation correlation violation")]
    ConversationCorrelation,
    #[error("active turn collision")]
    ActiveTurnCollision,
    #[error("generator protocol violation")]
    GeneratorProtocol,
}

/// Why a non-idempotent Written request could not be settled after a transport loss (§12.6, §16.1).
///
/// Closed enum; never free text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Error)]
#[serde(rename_all = "camelCase")]
pub enum UnknownOutcomeCause {
    /// The invocation hard deadline fired before any terminal.
    #[error("deadline exceeded")]
    DeadlineExceeded,
    /// A transport/business cancel was accepted but no safety terminal confirmed before the job died.
    #[error("cancellation unconfirmed")]
    CancellationUnconfirmed,
    /// The connection was lost (writer/reader/protocol fatal).
    #[error("connection lost")]
    ConnectionLost,
    /// The process exited without a terminal.
    #[error("process exited")]
    ProcessExited,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn transport_failure_stage_serializes_camelcase() {
        assert_eq!(
            serde_json::to_value(TransportFailureStage::RequestWrite)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("requestWrite")
        );
        assert_eq!(
            serde_json::to_value(TransportFailureStage::TransportCancelWrite)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("transportCancelWrite")
        );
        assert_eq!(
            serde_json::to_value(TransportFailureStage::ResponseRead)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("responseRead")
        );
        assert_eq!(
            serde_json::to_value(TransportFailureStage::SessionDrain)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("sessionDrain")
        );
    }

    #[test]
    fn fatal_settlement_cause_variants_project() {
        let lost = FatalSettlementCause::ConnectionLost {
            stage: TransportFailureStage::ResponseRead,
        };
        assert_eq!(
            serde_json::to_value(&lost).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "connectionLost": { "stage": "responseRead" } })
        );
        let exited = FatalSettlementCause::ProcessExited { exit_code: Some(1) };
        assert_eq!(
            serde_json::to_value(&exited).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "processExited": { "exitCode": 1 } })
        );
        // Unknown exit code omits the field (skip_serializing_if = Option::is_none).
        let unknown = FatalSettlementCause::ProcessExited { exit_code: None };
        assert_eq!(
            serde_json::to_value(&unknown).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "processExited": {} })
        );
    }

    #[test]
    fn agent_contract_failure_and_unknown_outcome_are_closed_camelcase_unions() {
        assert_eq!(
            serde_json::to_value(AgentContractFailure::ConversationCorrelation)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("conversationCorrelation")
        );
        assert_eq!(
            serde_json::to_value(AgentContractFailure::ActiveTurnCollision)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("activeTurnCollision")
        );
        assert_eq!(
            serde_json::to_value(UnknownOutcomeCause::DeadlineExceeded)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("deadlineExceeded")
        );
        assert_eq!(
            serde_json::to_value(UnknownOutcomeCause::CancellationUnconfirmed)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            json!("cancellationUnconfirmed")
        );
    }
}
