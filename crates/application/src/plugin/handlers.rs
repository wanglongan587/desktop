use crate::plugin::mapper::{build_plugin_from_discovered, map_plugin_to_contract};
use crate::plugin::ports::{PluginRepository, PluginScanner};
use crate::{ApplicationError, Clock};
use ora_contracts::{
    DisablePluginRequest, DisablePluginResponse, EnablePluginRequest, EnablePluginResponse,
    InstallPluginRequest, InstallPluginResponse, ListPluginsRequest, ListPluginsResponse,
    ScanPluginsRequest, ScanPluginsResponse, UninstallPluginRequest, UninstallPluginResponse,
};
use ora_domain::{AuditFields, Plugin, PluginId, PluginLifecycleState};

/// Handles scanning the plugins directory for installable manifests.
pub struct ScanPluginsHandler<Scanner> {
    scanner: Scanner,
}

impl<Scanner> ScanPluginsHandler<Scanner> {
    pub fn new(scanner: Scanner) -> Self {
        Self { scanner }
    }
}

impl<Scanner> ScanPluginsHandler<Scanner>
where
    Scanner: PluginScanner,
{
    /// Returns every plugin discovered by the scan, before installation.
    pub fn handle(
        &self,
        _request: ScanPluginsRequest,
    ) -> Result<ScanPluginsResponse, ApplicationError> {
        let plugins = self
            .scanner
            .scan()
            .map_err(ApplicationError::from_plugin_scanner_error)?;
        Ok(ScanPluginsResponse { plugins })
    }
}

/// Handles installing a discovered plugin by persisting its manifest.
pub struct InstallPluginHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> InstallPluginHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> InstallPluginHandler<Repository, ClockSource>
where
    Repository: PluginRepository,
    ClockSource: Clock,
{
    /// Persists a discovered plugin in the `Installed` state and returns its managed view.
    pub fn handle(
        &self,
        request: InstallPluginRequest,
    ) -> Result<InstallPluginResponse, ApplicationError> {
        let now = self.clock.now_timestamp_millis();
        let plugin = build_plugin_from_discovered(
            request.plugin,
            PluginLifecycleState::Installed,
            AuditFields::new(now, now, false),
        )?;
        let plugin = self
            .repository
            .create_plugin(plugin)
            .map_err(ApplicationError::from_plugin_repository_error)?;
        Ok(InstallPluginResponse {
            plugin: map_plugin_to_contract(plugin)?,
        })
    }
}

/// Handles listing installed plugins.
pub struct ListPluginsHandler<Repository> {
    repository: Repository,
}

impl<Repository> ListPluginsHandler<Repository> {
    pub fn new(repository: Repository) -> Self {
        Self { repository }
    }
}

impl<Repository> ListPluginsHandler<Repository>
where
    Repository: PluginRepository,
{
    /// Lists every installed plugin in deterministic order.
    pub fn handle(
        &self,
        _request: ListPluginsRequest,
    ) -> Result<ListPluginsResponse, ApplicationError> {
        let plugins = self
            .repository
            .list_plugins()
            .map_err(ApplicationError::from_plugin_repository_error)?;
        let plugins = plugins
            .into_iter()
            .map(map_plugin_to_contract)
            .collect::<Result<_, _>>()?;
        Ok(ListPluginsResponse { plugins })
    }
}

/// Handles enabling an installed plugin.
pub struct EnablePluginHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> EnablePluginHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> EnablePluginHandler<Repository, ClockSource>
where
    Repository: PluginRepository,
    ClockSource: Clock,
{
    /// Transitions a plugin to the `Enabled` state (idempotent if already enabled).
    pub fn handle(
        &self,
        request: EnablePluginRequest,
    ) -> Result<EnablePluginResponse, ApplicationError> {
        let plugin = transition_plugin_state(
            &self.repository,
            &self.clock,
            &request.plugin_id,
            PluginLifecycleState::Enabled,
        )?;
        Ok(EnablePluginResponse {
            plugin: map_plugin_to_contract(plugin)?,
        })
    }
}

/// Handles disabling an enabled plugin.
pub struct DisablePluginHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> DisablePluginHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> DisablePluginHandler<Repository, ClockSource>
where
    Repository: PluginRepository,
    ClockSource: Clock,
{
    /// Transitions a plugin back to the `Installed` state (idempotent if already installed).
    pub fn handle(
        &self,
        request: DisablePluginRequest,
    ) -> Result<DisablePluginResponse, ApplicationError> {
        let plugin = transition_plugin_state(
            &self.repository,
            &self.clock,
            &request.plugin_id,
            PluginLifecycleState::Installed,
        )?;
        Ok(DisablePluginResponse {
            plugin: map_plugin_to_contract(plugin)?,
        })
    }
}

/// Handles uninstalling (soft-deleting) a plugin.
pub struct UninstallPluginHandler<Repository, ClockSource> {
    repository: Repository,
    clock: ClockSource,
}

impl<Repository, ClockSource> UninstallPluginHandler<Repository, ClockSource> {
    pub fn new(repository: Repository, clock: ClockSource) -> Self {
        Self { repository, clock }
    }
}

impl<Repository, ClockSource> UninstallPluginHandler<Repository, ClockSource>
where
    Repository: PluginRepository,
    ClockSource: Clock,
{
    /// Soft-deletes a plugin and returns its identifier.
    pub fn handle(
        &self,
        request: UninstallPluginRequest,
    ) -> Result<UninstallPluginResponse, ApplicationError> {
        let plugin_id = PluginId::new(request.plugin_id);
        let deleted = self
            .repository
            .soft_delete_plugin(&plugin_id, self.clock.now_timestamp_millis())
            .map_err(ApplicationError::from_plugin_repository_error)?;
        if !deleted {
            return Err(ApplicationError::PluginNotFound {
                plugin_id: plugin_id.to_string(),
            });
        }
        Ok(UninstallPluginResponse {
            plugin_id: plugin_id.to_string(),
        })
    }
}

/// Finds a plugin, validates and persists a state transition, and returns the updated domain plugin.
///
/// Returns the plugin unchanged when it is already in the target state (idempotent).
fn transition_plugin_state<Repository, ClockSource>(
    repository: &Repository,
    clock: &ClockSource,
    request_plugin_id: &str,
    target: PluginLifecycleState,
) -> Result<Plugin, ApplicationError>
where
    Repository: PluginRepository,
    ClockSource: Clock,
{
    let plugin_id = PluginId::new(request_plugin_id.to_string());
    let mut plugin = repository
        .find_plugin(&plugin_id)
        .map_err(ApplicationError::from_plugin_repository_error)?
        .ok_or_else(|| ApplicationError::PluginNotFound {
            plugin_id: plugin_id.to_string(),
        })?;

    if plugin.state == target {
        return Ok(plugin);
    }

    let new_state = plugin
        .state
        .transition_to(target)
        .map_err(ApplicationError::from_plugin_domain_error)?;
    let now = clock.now_timestamp_millis();
    let updated = repository
        .update_state(&plugin_id, new_state, now)
        .map_err(ApplicationError::from_plugin_repository_error)?;
    if !updated {
        return Err(ApplicationError::PluginNotFound {
            plugin_id: plugin_id.to_string(),
        });
    }

    plugin.state = new_state;
    plugin.audit_fields = AuditFields::new(
        plugin.audit_fields.created_at,
        now,
        plugin.audit_fields.is_deleted,
    );
    Ok(plugin)
}
