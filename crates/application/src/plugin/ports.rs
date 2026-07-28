use ora_contracts::DiscoveredPlugin;
use ora_domain::{Plugin, PluginId, PluginLifecycleState};

/// Defines persistence operations required by plugin management use cases.
pub trait PluginRepository {
    /// Persists a newly installed plugin and returns it.
    fn create_plugin(&self, plugin: Plugin) -> Result<Plugin, PluginRepositoryError>;

    /// Loads one visible plugin by identifier.
    fn find_plugin(&self, plugin_id: &PluginId) -> Result<Option<Plugin>, PluginRepositoryError>;

    /// Lists visible plugins in deterministic storage order.
    fn list_plugins(&self) -> Result<Vec<Plugin>, PluginRepositoryError>;

    /// Replaces the lifecycle state of a visible plugin and reports whether a row matched.
    fn update_state(
        &self,
        plugin_id: &PluginId,
        state: PluginLifecycleState,
        updated_at: i64,
    ) -> Result<bool, PluginRepositoryError>;

    /// Marks a visible plugin deleted at the supplied timestamp.
    fn soft_delete_plugin(
        &self,
        plugin_id: &PluginId,
        deleted_at: i64,
    ) -> Result<bool, PluginRepositoryError>;
}

/// Scans a plugins directory and yields discovered manifests before installation.
pub trait PluginScanner {
    /// Returns every plugin discovered by scanning, each with its manifest and source path.
    fn scan(&self) -> Result<Vec<DiscoveredPlugin>, PluginScannerError>;
}

/// Represents plugin persistence failures exposed as stable application outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginRepositoryError {
    OperationFailed(String),
}

/// Represents plugin scanning failures exposed as stable application outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginScannerError {
    OperationFailed(String),
}
