use crate::PluginLifecycleState;
use thiserror::Error;

/// Enumerates domain-model conversion failures that adapters must handle explicitly.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainModelError {
    #[error("invalid project work context surface value: {0}")]
    InvalidProjectWorkContextSurface(String),
    #[error("invalid task status value: {0}")]
    InvalidTaskStatus(i64),
    #[error("invalid worktree activity value: {0}")]
    InvalidWorktreeActivity(i64),
    #[error("invalid virtual entry kind value: {0}")]
    InvalidVirtualEntryKind(i64),
    #[error("invalid session status value: {0}")]
    InvalidSessionStatus(i64),
    #[error("invalid agent CLI value: {0}")]
    InvalidAgentCli(String),
    #[error("skill name must not be blank")]
    EmptySkillName,
    #[error("agent definition name must not be blank")]
    EmptyAgentDefinitionName,
    #[error("invalid plugin lifecycle state value: {0}")]
    InvalidPluginLifecycleState(i64),
    #[error("invalid plugin kind value: {0}")]
    InvalidPluginKind(i64),
    #[error("invalid plugin state transition: {from_state:?} -> {to_state:?}")]
    InvalidPluginStateTransition {
        from_state: PluginLifecycleState,
        to_state: PluginLifecycleState,
    },
    #[error("plugin manifest version must not be blank")]
    EmptyPluginVersion,
}
