use crate::clock::SystemClock;
use ora_application::{
    ApplicationError, DisablePluginHandler, EnablePluginHandler, InstallPluginHandler,
    ListPluginsHandler, LocalDirPluginScanner, ScanPluginsHandler, UninstallPluginHandler,
};
use ora_contracts::{
    DisablePluginRequest, DisablePluginResponse, EnablePluginRequest, EnablePluginResponse,
    InstallPluginRequest, InstallPluginResponse, ListPluginsRequest, ListPluginsResponse,
    ScanPluginsRequest, ScanPluginsResponse, UninstallPluginRequest, UninstallPluginResponse,
};
use ora_db::{RepositoryPool, SqlitePluginRepository};
use std::path::PathBuf;

/// Groups the concrete plugin management handlers shared by runtime adapters.
///
/// Covers only persisted metadata and the local scan/install lifecycle; the live runtime
/// (spawn, plugin-channel, activation) lives in `ora-plugin-runtime` and is wired by the
/// Desktop adapter, not by this facade.
pub(crate) struct PluginApi {
    scan: ScanPluginsHandler<LocalDirPluginScanner>,
    install: InstallPluginHandler<SqlitePluginRepository, SystemClock>,
    list: ListPluginsHandler<SqlitePluginRepository>,
    enable: EnablePluginHandler<SqlitePluginRepository, SystemClock>,
    disable: DisablePluginHandler<SqlitePluginRepository, SystemClock>,
    uninstall: UninstallPluginHandler<SqlitePluginRepository, SystemClock>,
}

impl PluginApi {
    /// Builds plugin management handlers from the shared pool, clock, and plugins root.
    pub(crate) fn new(pool: RepositoryPool, clock: SystemClock, plugins_root: PathBuf) -> Self {
        let repository = SqlitePluginRepository::new(pool);
        let scanner = LocalDirPluginScanner::new(plugins_root);

        Self {
            scan: ScanPluginsHandler::new(scanner),
            install: InstallPluginHandler::new(repository.clone(), clock),
            list: ListPluginsHandler::new(repository.clone()),
            enable: EnablePluginHandler::new(repository.clone(), clock),
            disable: DisablePluginHandler::new(repository.clone(), clock),
            uninstall: UninstallPluginHandler::new(repository, clock),
        }
    }

    /// Executes plugin scanning through the application handler.
    pub(crate) fn scan(
        &self,
        request: ScanPluginsRequest,
    ) -> Result<ScanPluginsResponse, ApplicationError> {
        self.scan.handle(request)
    }

    /// Executes plugin installation through the application handler.
    pub(crate) fn install(
        &self,
        request: InstallPluginRequest,
    ) -> Result<InstallPluginResponse, ApplicationError> {
        self.install.handle(request)
    }

    /// Executes plugin listing through the application handler.
    pub(crate) fn list(
        &self,
        request: ListPluginsRequest,
    ) -> Result<ListPluginsResponse, ApplicationError> {
        self.list.handle(request)
    }

    /// Executes plugin enabling through the application handler.
    pub(crate) fn enable(
        &self,
        request: EnablePluginRequest,
    ) -> Result<EnablePluginResponse, ApplicationError> {
        self.enable.handle(request)
    }

    /// Executes plugin disabling through the application handler.
    pub(crate) fn disable(
        &self,
        request: DisablePluginRequest,
    ) -> Result<DisablePluginResponse, ApplicationError> {
        self.disable.handle(request)
    }

    /// Executes plugin uninstall through the application handler.
    pub(crate) fn uninstall(
        &self,
        request: UninstallPluginRequest,
    ) -> Result<UninstallPluginResponse, ApplicationError> {
        self.uninstall.handle(request)
    }
}
