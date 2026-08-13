use crate::plugin_api::security::PluginSecurity;
use crate::plugin_api::{InvocationRegistry, PluginBackend, PluginScopeResolver};
use crate::service::{FileSystemApi, WorkspaceFileApi};
use ora_backend::Backend;
use ora_plugin_manager::PluginManager;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Holds the shared state that HTTP handlers need to serve requests.
#[derive(Clone)]
pub struct AppState {
    backend: Backend,
    file_system_api: Arc<FileSystemApi>,
    workspace_file_api: Arc<WorkspaceFileApi>,
    plugin_manager: Arc<PluginManager>,
    plugin_scope_resolver: PluginScopeResolver,
    ready: Arc<AtomicBool>,
    plugin_backend: Option<Arc<dyn PluginBackend>>,
    plugin_security: Option<PluginSecurity>,
    plugin_invocations: InvocationRegistry,
}

impl AppState {
    /// Creates one shared application state value with readiness disabled until bootstrap completes.
    pub(crate) fn new(
        backend: Backend,
        file_system_api: Arc<FileSystemApi>,
        workspace_file_api: Arc<WorkspaceFileApi>,
        plugin_manager: Arc<PluginManager>,
        plugin_scope_resolver: PluginScopeResolver,
    ) -> Self {
        Self {
            backend,
            file_system_api,
            workspace_file_api,
            plugin_manager,
            plugin_scope_resolver,
            ready: Arc::new(AtomicBool::new(false)),
            plugin_backend: None,
            plugin_security: None,
            plugin_invocations: InvocationRegistry::default(),
        }
    }

    /// Installs the authenticated plugin facade only after backend readiness dependencies exist.
    pub(crate) fn with_plugin_backend(
        mut self,
        backend: Arc<dyn PluginBackend>,
        security: PluginSecurity,
    ) -> Self {
        self.plugin_backend = Some(backend);
        self.plugin_security = Some(security);
        self
    }

    pub(crate) fn plugin_backend(&self) -> Option<&Arc<dyn PluginBackend>> {
        self.plugin_backend.as_ref()
    }

    pub(crate) fn plugin_security(&self) -> Option<&PluginSecurity> {
        self.plugin_security.as_ref()
    }

    pub(crate) fn plugin_invocations(&self) -> &InvocationRegistry {
        &self.plugin_invocations
    }

    pub(crate) fn plugin_scope_resolver(&self) -> &PluginScopeResolver {
        &self.plugin_scope_resolver
    }

    /// Returns the shared persisted backend used by the five common CRUD route families.
    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    /// Returns the shared read-only filesystem API used by the web path picker.
    pub fn file_system_api(&self) -> &Arc<FileSystemApi> {
        &self.file_system_api
    }

    /// Returns the shared task-workspace filesystem API used by explorer and viewer routes.
    pub fn workspace_file_api(&self) -> &Arc<WorkspaceFileApi> {
        &self.workspace_file_api
    }
    /// Returns the immutable installed-plugin snapshot captured during bootstrap.
    pub fn plugin_manager(&self) -> &Arc<PluginManager> {
        &self.plugin_manager
    }

    /// Marks the runtime as ready after bootstrap finishes successfully.
    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::SeqCst);
    }

    /// Closes readiness before listener and plugin shutdown begins.
    pub fn mark_unready(&self) {
        self.ready.store(false, Ordering::SeqCst);
    }

    /// Stops every plugin generation through the application adapter if it is installed.
    pub async fn shutdown_plugins(&self) {
        if let Some(backend) = &self.plugin_backend {
            let _ = backend.shutdown().await;
        }
    }

    /// Cancels every authenticated invocation stream before waiting for HTTP connection drain.
    pub(crate) async fn cancel_plugin_invocations(&self) {
        self.plugin_invocations.cancel_all().await;
    }

    /// Reports whether bootstrap has completed successfully for readiness checks.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
}
