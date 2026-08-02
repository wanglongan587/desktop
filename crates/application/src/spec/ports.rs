use crate::RepositoryError;
use ora_domain::{ProjectId, SpecDocument, SpecSource, TaskId};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Reports why a spec request could not be mapped onto a workspace directory.
#[derive(Debug, Error)]
pub enum SpecWorkspaceError {
    #[error("project not found: {project_id}")]
    ProjectNotFound { project_id: String },
    #[error("task not found: {task_id}")]
    TaskNotFound { task_id: String },
    #[error("spec workspace is unavailable")]
    Unavailable {
        #[source]
        source: RepositoryError,
    },
}

/// Reports why an indexed spec catalog could not answer a request.
#[derive(Debug, Error)]
pub enum SpecCatalogError {
    #[error("spec workspace cannot be indexed")]
    WorkspaceUnavailable {
        #[source]
        source: RepositoryError,
    },
    #[error("spec not found: {path}")]
    DocumentNotFound { path: String },
}

/// Holds one consistent view of the specs discovered inside a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecCatalogSnapshot {
    pub workspace_root: PathBuf,
    pub sources: Vec<SpecSource>,
    pub documents: Vec<SpecDocument>,
}

/// Maps a spec request's scope onto the directory whose files it should describe.
///
/// The scope deliberately mirrors the workspace the user has selected rather than the
/// project as a whole: a task backed by a linked worktree sits on a different branch, so
/// its specs are a different set of files. Implementations resolve the project root when
/// no task is supplied and the task's own working directory otherwise.
pub trait SpecWorkspaceResolver {
    /// Resolves the on-disk directory whose specs a request targets.
    fn resolve_spec_workspace(
        &self,
        project_id: &ProjectId,
        task_id: Option<&TaskId>,
    ) -> Result<PathBuf, SpecWorkspaceError>;
}

/// Supplies the discovered spec catalog for a workspace directory.
///
/// Implementations own discovery, freshness and any caching. Handlers treat every call as
/// returning the current truth, so an implementation that caches must invalidate on its
/// own rather than expecting callers to ask for a refresh.
pub trait SpecCatalogReader {
    /// Returns the specs currently discoverable under one workspace root.
    fn snapshot(&self, workspace_root: &Path) -> Result<SpecCatalogSnapshot, SpecCatalogError>;

    /// Returns one catalogued document together with its raw body.
    fn read_document(
        &self,
        workspace_root: &Path,
        relative_path: &str,
    ) -> Result<(SpecDocument, String), SpecCatalogError>;
}
