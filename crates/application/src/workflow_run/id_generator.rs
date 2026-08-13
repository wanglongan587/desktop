use ora_domain::{WorkflowNodeRunId, WorkflowRunId};
use uuid::Uuid;

use super::engine::WorkflowNodeRunIdGenerator;
use super::ports::WorkflowRunIdGenerator;

/// Generates UUID-based identifiers for workflow runs.
#[derive(Clone, Debug, Default)]
pub struct UuidWorkflowRunIdGenerator;

impl UuidWorkflowRunIdGenerator {
    /// Creates a new UUID v4-based identifier generator.
    pub fn new() -> Self {
        Self
    }
}

impl WorkflowRunIdGenerator for UuidWorkflowRunIdGenerator {
    fn generate_run_id(&self) -> WorkflowRunId {
        WorkflowRunId::new(Uuid::new_v4().to_string())
    }
}

/// Generates UUID-based identifiers for node runs created by the engine.
#[derive(Clone, Debug, Default)]
pub struct UuidWorkflowNodeRunIdGenerator;

impl UuidWorkflowNodeRunIdGenerator {
    /// Creates a new UUID v4-based node-run identifier generator.
    pub fn new() -> Self {
        Self
    }
}

impl WorkflowNodeRunIdGenerator for UuidWorkflowNodeRunIdGenerator {
    fn generate_node_run_id(&self) -> WorkflowNodeRunId {
        WorkflowNodeRunId::new(Uuid::new_v4().to_string())
    }
}
