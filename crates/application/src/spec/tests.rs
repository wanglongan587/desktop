use crate::ApplicationError;
use crate::RepositoryError;
use crate::spec::handlers::{ListSpecsHandler, ReadSpecHandler};
use crate::spec::ports::{
    SpecCatalogError, SpecCatalogReader, SpecCatalogSnapshot, SpecWorkspaceError,
    SpecWorkspaceResolver,
};
use ora_contracts::{
    ListSpecsRequest, ListSpecsResponse, ReadSpecRequest, ReadSpecResponse, SpecDocument,
    SpecSource,
};
use ora_domain::{
    ProjectId, SpecContentHash, SpecDocument as DomainSpecDocument, SpecId, SpecIdentity, SpecPath,
    SpecSource as DomainSpecSource, TaskId,
};
use pretty_assertions::assert_eq;
use std::path::{Path, PathBuf};

/// Resolves scopes to fixed directories so handler behavior can be asserted without a filesystem.
struct StubResolver {
    project_root: PathBuf,
    task_root: PathBuf,
}

impl SpecWorkspaceResolver for StubResolver {
    fn resolve_spec_workspace(
        &self,
        _project_id: &ProjectId,
        task_id: Option<&TaskId>,
    ) -> Result<PathBuf, SpecWorkspaceError> {
        match task_id {
            Some(_) => Ok(self.task_root.clone()),
            None => Ok(self.project_root.clone()),
        }
    }
}

/// Fails every resolution so error translation can be asserted.
struct FailingResolver(SpecWorkspaceError);

impl SpecWorkspaceResolver for FailingResolver {
    fn resolve_spec_workspace(
        &self,
        _project_id: &ProjectId,
        _task_id: Option<&TaskId>,
    ) -> Result<PathBuf, SpecWorkspaceError> {
        match &self.0 {
            SpecWorkspaceError::ProjectNotFound { project_id } => {
                Err(SpecWorkspaceError::ProjectNotFound {
                    project_id: project_id.clone(),
                })
            }
            SpecWorkspaceError::TaskNotFound { task_id } => Err(SpecWorkspaceError::TaskNotFound {
                task_id: task_id.clone(),
            }),
            SpecWorkspaceError::Unavailable { .. } => Err(SpecWorkspaceError::Unavailable {
                source: RepositoryError::from_message("stub"),
            }),
        }
    }
}

/// Serves one catalog per workspace root so scoping can be observed through the handlers.
struct StubCatalog {
    project_snapshot: SpecCatalogSnapshot,
    task_snapshot: SpecCatalogSnapshot,
}

impl SpecCatalogReader for StubCatalog {
    fn snapshot(&self, workspace_root: &Path) -> Result<SpecCatalogSnapshot, SpecCatalogError> {
        for snapshot in [&self.project_snapshot, &self.task_snapshot] {
            if snapshot.workspace_root == workspace_root {
                return Ok(snapshot.clone());
            }
        }

        Err(SpecCatalogError::WorkspaceUnavailable {
            source: RepositoryError::from_message("unknown workspace"),
        })
    }

    fn read_document(
        &self,
        workspace_root: &Path,
        relative_path: &str,
    ) -> Result<(DomainSpecDocument, String), SpecCatalogError> {
        let snapshot = self.snapshot(workspace_root)?;
        snapshot
            .documents
            .into_iter()
            .find(|document| document.path.as_str() == relative_path)
            .map(|document| (document, "# Body\n".to_string()))
            .ok_or_else(|| SpecCatalogError::DocumentNotFound {
                path: relative_path.to_string(),
            })
    }
}

/// Verifies listing projects the catalog into the contract view without a task scope.
#[test]
fn lists_project_scoped_specs() {
    let handler = ListSpecsHandler::new(stub_resolver(), stub_catalog());

    assert_eq!(
        handler
            .handle(ListSpecsRequest {
                project_id: "project-1".to_string(),
                task_id: None,
            })
            .unwrap_or_else(|error| panic!("list specs: {error}")),
        ListSpecsResponse {
            workspace_root: project_root().to_string_lossy().into_owned(),
            sources: vec![SpecSource {
                name: "Docs".to_string(),
                glob: "docs/specs/**/*.md".to_string(),
            }],
            specs: vec![SpecDocument {
                id: "add-auth".to_string(),
                source_name: "Docs".to_string(),
                path: "docs/specs/design.md".to_string(),
                title: "Design".to_string(),
            }],
        }
    );
}

/// Verifies a task scope selects that task's workspace instead of the project root.
#[test]
fn lists_task_scoped_specs_from_the_task_workspace() {
    let handler = ListSpecsHandler::new(stub_resolver(), stub_catalog());

    assert_eq!(
        handler
            .handle(ListSpecsRequest {
                project_id: "project-1".to_string(),
                task_id: Some("task-1".to_string()),
            })
            .unwrap_or_else(|error| panic!("list specs: {error}")),
        ListSpecsResponse {
            workspace_root: task_root().to_string_lossy().into_owned(),
            sources: vec![SpecSource {
                name: "Docs".to_string(),
                glob: "docs/specs/**/*.md".to_string(),
            }],
            specs: vec![SpecDocument {
                id: "docs/specs/branch-only.md".to_string(),
                source_name: "Docs".to_string(),
                path: "docs/specs/branch-only.md".to_string(),
                title: "Branch only".to_string(),
            }],
        }
    );
}

/// Verifies reading returns the document view together with its body.
#[test]
fn reads_one_document_with_its_body() {
    let handler = ReadSpecHandler::new(stub_resolver(), stub_catalog());

    assert_eq!(
        handler
            .handle(ReadSpecRequest {
                project_id: "project-1".to_string(),
                task_id: None,
                path: "docs/specs/design.md".to_string(),
            })
            .unwrap_or_else(|error| panic!("read spec: {error}")),
        ReadSpecResponse {
            spec: SpecDocument {
                id: "add-auth".to_string(),
                source_name: "Docs".to_string(),
                path: "docs/specs/design.md".to_string(),
                title: "Design".to_string(),
            },
            content: "# Body\n".to_string(),
        }
    );
}

/// Verifies an uncatalogued path fails as a missing spec rather than reaching the filesystem.
#[test]
fn rejects_uncatalogued_documents() {
    let handler = ReadSpecHandler::new(stub_resolver(), stub_catalog());

    assert_eq!(
        handler
            .handle(ReadSpecRequest {
                project_id: "project-1".to_string(),
                task_id: None,
                path: "secrets.env".to_string(),
            })
            .expect_err("uncatalogued paths must be rejected"),
        ApplicationError::SpecNotFound {
            path: "secrets.env".to_string(),
        }
    );
}

/// Verifies scope resolution failures keep their existing application semantics.
#[test]
fn preserves_scope_resolution_semantics() {
    let missing_project = ListSpecsHandler::new(
        FailingResolver(SpecWorkspaceError::ProjectNotFound {
            project_id: "project-1".to_string(),
        }),
        stub_catalog(),
    );
    let missing_task = ListSpecsHandler::new(
        FailingResolver(SpecWorkspaceError::TaskNotFound {
            task_id: "task-1".to_string(),
        }),
        stub_catalog(),
    );

    assert_eq!(
        missing_project
            .handle(ListSpecsRequest {
                project_id: "project-1".to_string(),
                task_id: None,
            })
            .expect_err("missing project must fail"),
        ApplicationError::ProjectNotFound {
            project_id: "project-1".to_string(),
        }
    );
    assert_eq!(
        missing_task
            .handle(ListSpecsRequest {
                project_id: "project-1".to_string(),
                task_id: Some("task-1".to_string()),
            })
            .expect_err("missing task must fail"),
        ApplicationError::TaskNotFound {
            task_id: "task-1".to_string(),
        }
    );
}

/// Builds the resolver used by every handler fixture.
fn stub_resolver() -> StubResolver {
    StubResolver {
        project_root: project_root(),
        task_root: task_root(),
    }
}

/// Builds a catalog whose two workspaces hold deliberately different documents.
fn stub_catalog() -> StubCatalog {
    let sources = vec![DomainSpecSource::new("Docs", "docs/specs/**/*.md")];

    StubCatalog {
        project_snapshot: SpecCatalogSnapshot {
            workspace_root: project_root(),
            sources: sources.clone(),
            documents: vec![DomainSpecDocument::new(
                SpecIdentity::Declared(SpecId::new("add-auth")),
                "Docs",
                SpecPath::from_relative(Path::new("docs/specs/design.md")),
                "Design",
                SpecContentHash::new("hash-1"),
            )],
        },
        task_snapshot: SpecCatalogSnapshot {
            workspace_root: task_root(),
            sources,
            documents: vec![DomainSpecDocument::new(
                SpecIdentity::Derived(SpecId::new("docs/specs/branch-only.md")),
                "Docs",
                SpecPath::from_relative(Path::new("docs/specs/branch-only.md")),
                "Branch only",
                SpecContentHash::new("hash-2"),
            )],
        },
    }
}

/// Returns the directory standing in for a project root.
fn project_root() -> PathBuf {
    PathBuf::from("workspace").join("ora")
}

/// Returns the directory standing in for a task-owned worktree.
fn task_root() -> PathBuf {
    PathBuf::from("workspace").join("worktrees").join("task-1")
}
