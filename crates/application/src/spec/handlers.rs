use crate::ApplicationError;
use crate::spec::mapper::{map_spec_document, map_spec_source};
use crate::spec::ports::{
    SpecCatalogError, SpecCatalogReader, SpecWorkspaceError, SpecWorkspaceResolver,
};
use ora_contracts::{ListSpecsRequest, ListSpecsResponse, ReadSpecRequest, ReadSpecResponse};
use ora_domain::{ProjectId, TaskId};
use std::path::PathBuf;

/// Lists every spec discovered in the workspace a request scopes itself to.
pub struct ListSpecsHandler<Resolver, Catalog> {
    resolver: Resolver,
    catalog: Catalog,
}

impl<Resolver, Catalog> ListSpecsHandler<Resolver, Catalog> {
    pub fn new(resolver: Resolver, catalog: Catalog) -> Self {
        Self { resolver, catalog }
    }
}

impl<Resolver, Catalog> ListSpecsHandler<Resolver, Catalog>
where
    Resolver: SpecWorkspaceResolver,
    Catalog: SpecCatalogReader,
{
    /// Resolves the request's workspace and returns its catalog in configuration order.
    pub fn handle(&self, request: ListSpecsRequest) -> Result<ListSpecsResponse, ApplicationError> {
        let workspace_root =
            resolve_workspace(&self.resolver, &request.project_id, request.task_id)?;
        let snapshot = self
            .catalog
            .snapshot(&workspace_root)
            .map_err(ApplicationError::from_spec_catalog_error)?;

        Ok(ListSpecsResponse {
            workspace_root: snapshot.workspace_root.to_string_lossy().into_owned(),
            sources: snapshot.sources.into_iter().map(map_spec_source).collect(),
            specs: snapshot
                .documents
                .into_iter()
                .map(map_spec_document)
                .collect(),
        })
    }
}

/// Reads the body of one spec discovered in the workspace a request scopes itself to.
pub struct ReadSpecHandler<Resolver, Catalog> {
    resolver: Resolver,
    catalog: Catalog,
}

impl<Resolver, Catalog> ReadSpecHandler<Resolver, Catalog> {
    pub fn new(resolver: Resolver, catalog: Catalog) -> Self {
        Self { resolver, catalog }
    }
}

impl<Resolver, Catalog> ReadSpecHandler<Resolver, Catalog>
where
    Resolver: SpecWorkspaceResolver,
    Catalog: SpecCatalogReader,
{
    /// Returns one document's metadata and body, or a stable not-found error.
    pub fn handle(&self, request: ReadSpecRequest) -> Result<ReadSpecResponse, ApplicationError> {
        let workspace_root =
            resolve_workspace(&self.resolver, &request.project_id, request.task_id)?;
        let (document, content) = self
            .catalog
            .read_document(&workspace_root, &request.path)
            .map_err(ApplicationError::from_spec_catalog_error)?;

        Ok(ReadSpecResponse {
            spec: map_spec_document(document),
            content,
        })
    }
}

/// Resolves a request's scope into a workspace directory.
///
/// Shared by both handlers because the scoping rule, not the operation, decides which
/// directory a spec request describes.
fn resolve_workspace<Resolver>(
    resolver: &Resolver,
    project_id: &str,
    task_id: Option<String>,
) -> Result<PathBuf, ApplicationError>
where
    Resolver: SpecWorkspaceResolver,
{
    let project_id = ProjectId::new(project_id);
    let task_id = task_id.map(TaskId::new);

    resolver
        .resolve_spec_workspace(&project_id, task_id.as_ref())
        .map_err(ApplicationError::from_spec_workspace_error)
}

impl ApplicationError {
    /// Maps workspace resolution failures onto their existing application semantics.
    fn from_spec_workspace_error(error: SpecWorkspaceError) -> Self {
        match error {
            SpecWorkspaceError::ProjectNotFound { project_id } => {
                Self::ProjectNotFound { project_id }
            }
            SpecWorkspaceError::TaskNotFound { task_id } => Self::TaskNotFound { task_id },
            SpecWorkspaceError::Unavailable { source } => Self::SpecWorkspaceUnavailable { source },
        }
    }

    /// Maps catalog failures onto stable application errors.
    fn from_spec_catalog_error(error: SpecCatalogError) -> Self {
        match error {
            SpecCatalogError::WorkspaceUnavailable { source } => {
                Self::SpecWorkspaceUnavailable { source }
            }
            SpecCatalogError::DocumentNotFound { path } => Self::SpecNotFound { path },
        }
    }
}
