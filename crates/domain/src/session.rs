use crate::{AuditFields, DomainModelError, TaskId};
use serde::{Deserialize, Serialize};

/// Identifies the application-scoped CLI process that owns a provider session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentCli {
    OpenCode,
    Nga,
    CodeAgentCli,
}

impl AgentCli {
    pub const ALL: [Self; 3] = [Self::OpenCode, Self::Nga, Self::CodeAgentCli];

    /// Returns the namespaced text persisted independently of enum declaration order.
    pub fn database_value(self) -> &'static str {
        match self {
            Self::OpenCode => "ora-space.opencode",
            Self::Nga => "ora-space.nga",
            Self::CodeAgentCli => "ora-space.codeagentcli",
        }
    }

    /// Restores a CLI identity while rejecting unknown persisted namespaces.
    pub fn from_database_value(value: &str) -> Result<Self, DomainModelError> {
        match value {
            "ora-space.opencode" => Ok(Self::OpenCode),
            "ora-space.nga" => Ok(Self::Nga),
            "ora-space.codeagentcli" => Ok(Self::CodeAgentCli),
            _ => Err(DomainModelError::InvalidAgentCli(value.to_string())),
        }
    }

    /// Returns the executable basename used by PATH lookup on Windows.
    pub fn executable_name(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::Nga => "nga",
            Self::CodeAgentCli => "codeagentcli",
        }
    }
}

/// Captures whether a conversation is registered on its shared CLI connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    Running,
    Stopped,
}

impl SessionStatus {
    /// Returns the integer code used by persistence adapters for this session status.
    pub fn database_value(self) -> i64 {
        match self {
            Self::Running => 0,
            Self::Stopped => 1,
        }
    }

    /// Converts a persisted integer into a strongly typed session status.
    pub fn from_database_value(value: i64) -> Result<Self, DomainModelError> {
        match value {
            0 => Ok(Self::Running),
            1 => Ok(Self::Stopped),
            _ => Err(DomainModelError::InvalidSessionStatus(value)),
        }
    }
}

/// Represents one provider-backed conversation whose routing fields are immutable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: crate::SessionId,
    pub task_id: TaskId,
    pub agent_cli: AgentCli,
    pub agent_session_id: String,
    pub status: SessionStatus,
    pub audit_fields: AuditFields,
}

impl Session {
    /// Creates a session only after the provider has returned its required session identifier.
    pub fn new(
        id: crate::SessionId,
        task_id: TaskId,
        agent_cli: AgentCli,
        agent_session_id: impl Into<String>,
        status: SessionStatus,
        audit_fields: AuditFields,
    ) -> Self {
        Self {
            id,
            task_id,
            agent_cli,
            agent_session_id: agent_session_id.into(),
            status,
            audit_fields,
        }
    }

    /// Changes only registration state while preserving immutable provider routing.
    pub fn with_status(mut self, status: SessionStatus, updated_at: i64) -> Self {
        self.status = status;
        self.audit_fields.updated_at = updated_at;
        self
    }
}
