use ora_application::{
    ApplicationError, ListSpecsHandler, ProjectRepository, ReadSpecHandler, RepositoryError,
    SpecCatalogError, SpecCatalogReader, SpecCatalogSnapshot, SpecWorkspaceError,
    SpecWorkspaceResolver,
};
use ora_contracts::{ListSpecsRequest, ListSpecsResponse, ReadSpecRequest, ReadSpecResponse};
use ora_db::{RepositoryPool, SqliteProjectRepository};
use ora_domain::{ProjectId, SpecDocument, TaskId};
use ora_spec::SpecCatalog;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Groups the concrete spec handlers shared by runtime adapters.
pub(crate) struct SpecApi {
    list: ListSpecsHandler<PoolSpecWorkspaceResolver, SharedSpecCatalog>,
    read: ReadSpecHandler<PoolSpecWorkspaceResolver, SharedSpecCatalog>,
}

impl SpecApi {
    /// Builds spec handlers over one shared catalog.
    ///
    /// Both handlers observe the same catalog instance so the watcher started by a listing
    /// keeps a subsequent read on the same indexed state.
    pub(crate) fn new(pool: RepositoryPool) -> Self {
        let resolver = PoolSpecWorkspaceResolver { pool };
        let catalog = SharedSpecCatalog(Arc::new(SpecCatalog::new(Vec::new())));

        Self {
            list: ListSpecsHandler::new(resolver.clone(), catalog.clone()),
            read: ReadSpecHandler::new(resolver, catalog),
        }
    }

    /// Executes spec listing through the application handler.
    pub(crate) fn list(
        &self,
        request: ListSpecsRequest,
    ) -> Result<ListSpecsResponse, ApplicationError> {
        self.list.handle(request)
    }

    /// Executes one spec read through the application handler.
    pub(crate) fn read(
        &self,
        request: ReadSpecRequest,
    ) -> Result<ReadSpecResponse, ApplicationError> {
        self.read.handle(request)
    }
}

/// Resolves spec scopes against persisted projects and live task workspaces.
#[derive(Clone)]
struct PoolSpecWorkspaceResolver {
    pool: RepositoryPool,
}

impl SpecWorkspaceResolver for PoolSpecWorkspaceResolver {
    /// Maps a scope onto its directory, reusing the same resolution agents run in.
    ///
    /// Delegating the task branch to `resolve_task_cwd` is deliberate: a task's specs must
    /// come from the exact directory its sessions execute in, whether that is a linked
    /// worktree on another branch or the project root itself.
    fn resolve_spec_workspace(
        &self,
        project_id: &ProjectId,
        task_id: Option<&TaskId>,
    ) -> Result<PathBuf, SpecWorkspaceError> {
        match task_id {
            Some(task_id) => crate::task::resolve_task_cwd(&self.pool, task_id).map_err(|source| {
                SpecWorkspaceError::Unavailable {
                    source: RepositoryError::new(source),
                }
            }),
            None => self.resolve_project_root(project_id),
        }
    }
}

impl PoolSpecWorkspaceResolver {
    /// Loads one project's repository root as an absolute directory.
    fn resolve_project_root(&self, project_id: &ProjectId) -> Result<PathBuf, SpecWorkspaceError> {
        let project = SqliteProjectRepository::new(self.pool.clone())
            .find_project(project_id)
            .map_err(|source| SpecWorkspaceError::Unavailable { source })?
            .ok_or_else(|| SpecWorkspaceError::ProjectNotFound {
                project_id: project_id.to_string(),
            })?;
        let root = PathBuf::from(project.root_path);

        if root.is_absolute() {
            return Ok(root);
        }

        // A relative root only occurs for records written before roots were validated;
        // anchoring it to the running process keeps such a project usable rather than
        // silently scanning an unrelated directory.
        std::env::current_dir()
            .map(|current| current.join(root))
            .map_err(|source| SpecWorkspaceError::Unavailable {
                source: RepositoryError::new(source),
            })
    }
}

/// Shares one indexed catalog between the listing and reading use cases.
#[derive(Clone)]
struct SharedSpecCatalog(Arc<SpecCatalog>);

impl SpecCatalogReader for SharedSpecCatalog {
    fn snapshot(&self, workspace_root: &Path) -> Result<SpecCatalogSnapshot, SpecCatalogError> {
        let snapshot = self.0.snapshot(workspace_root).map_err(|source| {
            SpecCatalogError::WorkspaceUnavailable {
                source: RepositoryError::new(source),
            }
        })?;

        Ok(SpecCatalogSnapshot {
            workspace_root: snapshot.workspace_root,
            sources: snapshot.sources,
            documents: snapshot.documents,
        })
    }

    fn read_document(
        &self,
        workspace_root: &Path,
        relative_path: &str,
    ) -> Result<(SpecDocument, String), SpecCatalogError> {
        self.0
            .read_document(workspace_root, relative_path)
            .map_err(|source| match source {
                ora_spec::SpecError::DocumentNotFound { path } => {
                    SpecCatalogError::DocumentNotFound { path }
                }
                other => SpecCatalogError::WorkspaceUnavailable {
                    source: RepositoryError::new(other),
                },
            })
    }
}
