use crate::error::WebApiError;
use ora_application::{ProjectRepository, TaskRepository, WorktreeRepository};
use ora_contracts::ApplicationAgentScope;
use ora_db::{
    RepositoryPool, SqliteProjectRepository, SqliteTaskRepository, SqliteWorktreeRepository,
};
use ora_domain::{ProjectId, TaskId, WorktreeActivity, WorktreeId};
use ora_plugin_protocol::{AgentScope, HostResolvedAbsolutePath, ProjectHandle, WorktreeHandle};
use std::path::{Path, PathBuf};

/// Resolves transport-visible application ids into current persisted ownership and canonical paths.
#[derive(Clone)]
pub(crate) struct PluginScopeResolver {
    pool: RepositoryPool,
    work_dir: PathBuf,
}

impl PluginScopeResolver {
    pub(crate) fn new(pool: RepositoryPool, work_dir: PathBuf) -> Self {
        Self { pool, work_dir }
    }

    /// Performs blocking repository and filesystem checks away from the async HTTP executor.
    pub(crate) async fn resolve(
        &self,
        scope: ApplicationAgentScope,
    ) -> Result<AgentScope, WebApiError> {
        let resolver = self.clone();
        tokio::task::spawn_blocking(move || resolver.resolve_blocking(scope))
            .await
            .map_err(|error| {
                WebApiError::internal("agent scope resolver stopped unexpectedly", error)
            })?
    }

    /// Revalidates object membership on every invocation before any plugin frame can be written.
    fn resolve_blocking(&self, scope: ApplicationAgentScope) -> Result<AgentScope, WebApiError> {
        match scope {
            ApplicationAgentScope::Global {} => Ok(AgentScope::Global {}),
            ApplicationAgentScope::Project { project_id } => {
                let project_id = ProjectId::new(project_id);
                let project = SqliteProjectRepository::new(self.pool.clone())
                    .find_project(&project_id)
                    .map_err(|error| WebApiError::internal("project scope lookup failed", error))?
                    .ok_or_else(invalid_scope)?;
                let working_directory = canonical_host_path(Path::new(&project.root_path))?;
                Ok(AgentScope::Project {
                    project_handle: ProjectHandle::parse(project.id.to_string())
                        .map_err(|_| invalid_scope())?,
                    working_directory,
                })
            }
            ApplicationAgentScope::Worktree {
                project_id,
                worktree_id,
            } => {
                let project_id = ProjectId::new(project_id);
                let worktree_id = WorktreeId::new(worktree_id);
                let worktree = SqliteWorktreeRepository::new(self.pool.clone())
                    .find_worktree(&worktree_id)
                    .map_err(|error| WebApiError::internal("worktree scope lookup failed", error))?
                    .ok_or_else(invalid_scope)?;
                let task = SqliteTaskRepository::new(self.pool.clone())
                    .find_task(&TaskId::new(worktree.task_id.to_string()))
                    .map_err(|error| WebApiError::internal("worktree task lookup failed", error))?
                    .ok_or_else(invalid_scope)?;
                if task.project_id != project_id
                    || task.worktree_id.as_ref() != Some(&worktree_id)
                    || worktree.activity != WorktreeActivity::Active
                {
                    return Err(invalid_scope());
                }
                let project = SqliteProjectRepository::new(self.pool.clone())
                    .find_project(&project_id)
                    .map_err(|error| {
                        WebApiError::internal("worktree project lookup failed", error)
                    })?
                    .ok_or_else(invalid_scope)?;
                let managed_root =
                    std::fs::canonicalize(&self.work_dir).map_err(|_| invalid_scope())?;
                let working_path = std::fs::canonicalize(self.work_dir.join(task.id.to_string()))
                    .map_err(|_| invalid_scope())?;
                if !working_path.starts_with(&managed_root) {
                    return Err(invalid_scope());
                }
                Ok(AgentScope::Worktree {
                    project_handle: ProjectHandle::parse(project.id.to_string())
                        .map_err(|_| invalid_scope())?,
                    worktree_handle: WorktreeHandle::parse(worktree.id.to_string())
                        .map_err(|_| invalid_scope())?,
                    working_directory: host_path_from_canonical(working_path)?,
                })
            }
        }
    }
}

/// Canonicalizes one Host-owned directory and converts it into the protocol's validated path leaf.
fn canonical_host_path(path: &Path) -> Result<HostResolvedAbsolutePath, WebApiError> {
    let canonical = std::fs::canonicalize(path).map_err(|_| invalid_scope())?;
    host_path_from_canonical(canonical)
}

/// Preserves the canonical Windows representation without accepting a client-authored path.
fn host_path_from_canonical(path: PathBuf) -> Result<HostResolvedAbsolutePath, WebApiError> {
    HostResolvedAbsolutePath::parse(path.to_string_lossy().into_owned())
        .map_err(|_| invalid_scope())
}

fn invalid_scope() -> WebApiError {
    WebApiError::bad_request("agent scope is invalid or no longer available")
}

#[cfg(all(test, windows))]
mod tests {
    use super::PluginScopeResolver;
    use ora_application::{ProjectRepository, TaskRepository, WorktreeRepository};
    use ora_contracts::ApplicationAgentScope;
    use ora_db::{
        DatabaseBootstrapper, DatabaseLocation, SqliteProjectRepository, SqliteTaskRepository,
        SqliteWorktreeRepository, default_migration_catalog,
    };
    use ora_domain::{
        AuditFields, Project, ProjectId, Task, TaskId, TaskStatus, Worktree, WorktreeActivity,
        WorktreeBaseline, WorktreeId,
    };
    use ora_plugin_protocol::{
        AgentScope, HostResolvedAbsolutePath, ProjectHandle, WorktreeHandle,
    };
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    /// Resolves only persisted project/worktree membership and never consumes a client path.
    #[tokio::test]
    async fn resolves_current_project_and_worktree_membership() {
        let root = TempDir::new().unwrap_or_else(|error| panic!("expected test root: {error}"));
        let project_root = root.path().join("project");
        let work_dir = root.path().join("worktrees");
        let project_id = ProjectId::new("project-1");
        let task_id = TaskId::new("task-1");
        let worktree_id = WorktreeId::new("worktree-1");
        std::fs::create_dir_all(&project_root)
            .unwrap_or_else(|error| panic!("expected project root: {error}"));
        std::fs::create_dir_all(work_dir.join(task_id.to_string()))
            .unwrap_or_else(|error| panic!("expected worktree root: {error}"));
        let pool = DatabaseBootstrapper::system()
            .bootstrap_repository_pool(
                &DatabaseLocation::path(root.path().join("scope.sqlite3")),
                &default_migration_catalog()
                    .unwrap_or_else(|error| panic!("expected migration catalog: {error}")),
            )
            .unwrap_or_else(|error| panic!("expected repository pool: {error}"));
        let audit = AuditFields::new(1, 1, false);
        SqliteProjectRepository::new(pool.clone())
            .create_project(Project::new(
                project_id.clone(),
                "Project",
                project_root.to_string_lossy(),
                audit.clone(),
            ))
            .unwrap_or_else(|error| panic!("expected project: {error:?}"));
        SqliteTaskRepository::new(pool.clone())
            .create_task(Task::new(
                task_id.clone(),
                project_id.clone(),
                "Task",
                TaskStatus::Doing,
                Some(worktree_id.clone()),
                audit.clone(),
            ))
            .unwrap_or_else(|error| panic!("expected task: {error:?}"));
        SqliteWorktreeRepository::new(pool.clone())
            .create_worktree(Worktree::new(
                worktree_id.clone(),
                task_id.clone(),
                Some("task-branch".to_owned()),
                None,
                WorktreeBaseline::unavailable(),
                WorktreeActivity::Active,
                audit,
            ))
            .unwrap_or_else(|error| panic!("expected worktree: {error:?}"));
        let resolver = PluginScopeResolver::new(pool, work_dir.clone());

        let project = resolver
            .resolve(ApplicationAgentScope::Project {
                project_id: project_id.to_string(),
            })
            .await
            .unwrap_or_else(|_| panic!("expected project scope"));
        let worktree = resolver
            .resolve(ApplicationAgentScope::Worktree {
                project_id: project_id.to_string(),
                worktree_id: worktree_id.to_string(),
            })
            .await
            .unwrap_or_else(|_| panic!("expected worktree scope"));

        assert_eq!(
            (project, worktree),
            (
                AgentScope::Project {
                    project_handle: ProjectHandle::parse(project_id.to_string())
                        .unwrap_or_else(|error| panic!("expected project handle: {error}")),
                    working_directory: HostResolvedAbsolutePath::parse(
                        std::fs::canonicalize(project_root)
                            .unwrap_or_else(|error| panic!("expected canonical project: {error}"))
                            .to_string_lossy()
                            .into_owned(),
                    )
                    .unwrap_or_else(|error| panic!("expected project path: {error}")),
                },
                AgentScope::Worktree {
                    project_handle: ProjectHandle::parse(project_id.to_string())
                        .unwrap_or_else(|error| panic!("expected project handle: {error}")),
                    worktree_handle: WorktreeHandle::parse(worktree_id.to_string())
                        .unwrap_or_else(|error| panic!("expected worktree handle: {error}")),
                    working_directory: HostResolvedAbsolutePath::parse(
                        std::fs::canonicalize(work_dir.join(task_id.to_string()))
                            .unwrap_or_else(|error| panic!("expected canonical worktree: {error}"))
                            .to_string_lossy()
                            .into_owned(),
                    )
                    .unwrap_or_else(|error| panic!("expected worktree path: {error}")),
                },
            )
        );
    }
}
