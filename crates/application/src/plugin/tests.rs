use super::{
    DisablePluginHandler, EnablePluginHandler, InstallPluginHandler, ListPluginsHandler,
    PluginRepository, PluginRepositoryError, PluginScanner, PluginScannerError, ScanPluginsHandler,
    UninstallPluginHandler,
};
use crate::{ApplicationError, Clock};
use ora_contracts::{
    DisablePluginRequest, DiscoveredPlugin, EnablePluginRequest, InstallPluginRequest,
    ListPluginsRequest, PluginKind, PluginManifest, PluginProcessEntrypoint, PluginState,
    ScanPluginsRequest, UninstallPluginRequest,
};
use ora_domain::{Plugin, PluginId, PluginLifecycleState};
use pretty_assertions::assert_eq;
use std::cell::RefCell;
use std::rc::Rc;

/// Exercises scan → install → enable (idempotent) → disable → uninstall and the not-found path.
#[test]
fn scans_installs_enables_disables_and_uninstalls_a_plugin() {
    let repository = Rc::new(FakePluginRepository::default());
    let scanner = Rc::new(FakePluginScanner::new(vec![discovered("codex")]));

    let scanned = ScanPluginsHandler::new(scanner.clone())
        .handle(ScanPluginsRequest {})
        .unwrap();
    assert_eq!(scanned.plugins.len(), 1);
    assert_eq!(scanned.plugins[0].manifest.id, "codex");

    let installed = InstallPluginHandler::new(repository.clone(), FixedClock(100))
        .handle(InstallPluginRequest {
            plugin: discovered("codex"),
        })
        .unwrap();
    assert_eq!(installed.plugin.id, "codex");
    assert_eq!(installed.plugin.state, PluginState::Installed);

    let listed = ListPluginsHandler::new(repository.clone())
        .handle(ListPluginsRequest {})
        .unwrap();
    assert_eq!(listed.plugins.len(), 1);

    let enabled = EnablePluginHandler::new(repository.clone(), FixedClock(200))
        .handle(EnablePluginRequest {
            plugin_id: "codex".to_string(),
        })
        .unwrap();
    assert_eq!(enabled.plugin.state, PluginState::Enabled);

    // Idempotent: enabling an already-enabled plugin does not error.
    let enabled_again = EnablePluginHandler::new(repository.clone(), FixedClock(201))
        .handle(EnablePluginRequest {
            plugin_id: "codex".to_string(),
        })
        .unwrap();
    assert_eq!(enabled_again.plugin.state, PluginState::Enabled);

    let disabled = DisablePluginHandler::new(repository.clone(), FixedClock(300))
        .handle(DisablePluginRequest {
            plugin_id: "codex".to_string(),
        })
        .unwrap();
    assert_eq!(disabled.plugin.state, PluginState::Installed);

    let uninstalled = UninstallPluginHandler::new(repository.clone(), FixedClock(400))
        .handle(UninstallPluginRequest {
            plugin_id: "codex".to_string(),
        })
        .unwrap();
    assert_eq!(uninstalled.plugin_id, "codex");

    // After uninstall the plugin is gone; enabling reports not found.
    let missing = EnablePluginHandler::new(repository, FixedClock(500))
        .handle(EnablePluginRequest {
            plugin_id: "codex".to_string(),
        })
        .unwrap_err();
    assert_eq!(
        missing,
        ApplicationError::PluginNotFound {
            plugin_id: "codex".to_string()
        }
    );
}

/// Verifies enabling a plugin that was never installed reports not found.
#[test]
fn reports_plugin_not_found_for_missing_plugin() {
    let repository = Rc::new(FakePluginRepository::default());
    let missing = EnablePluginHandler::new(repository, FixedClock(1))
        .handle(EnablePluginRequest {
            plugin_id: "ghost".to_string(),
        })
        .unwrap_err();
    assert_eq!(
        missing,
        ApplicationError::PluginNotFound {
            plugin_id: "ghost".to_string()
        }
    );
}

fn discovered(id: &str) -> DiscoveredPlugin {
    DiscoveredPlugin {
        manifest: PluginManifest {
            id: id.to_string(),
            version: "0.1.0".to_string(),
            kind: PluginKind::Agent,
            entrypoint: PluginProcessEntrypoint {
                program: "node".to_string(),
                args: vec!["adapter.js".to_string()],
                cwd: None,
                envs: Vec::new(),
            },
            display_name: id.to_string(),
            description: "test plugin".to_string(),
        },
        source_path: format!("plugins/{id}"),
    }
}

#[derive(Default)]
struct FakePluginRepository {
    plugins: RefCell<Vec<Plugin>>,
}

impl PluginRepository for Rc<FakePluginRepository> {
    fn create_plugin(&self, plugin: Plugin) -> Result<Plugin, PluginRepositoryError> {
        if self
            .plugins
            .borrow()
            .iter()
            .any(|existing| existing.id == plugin.id && !existing.audit_fields.is_deleted)
        {
            return Err(PluginRepositoryError::OperationFailed(
                "duplicate plugin id".to_string(),
            ));
        }
        self.plugins.borrow_mut().push(plugin.clone());
        Ok(plugin)
    }

    fn find_plugin(&self, plugin_id: &PluginId) -> Result<Option<Plugin>, PluginRepositoryError> {
        Ok(self
            .plugins
            .borrow()
            .iter()
            .find(|existing| existing.id == *plugin_id && !existing.audit_fields.is_deleted)
            .cloned())
    }

    fn list_plugins(&self) -> Result<Vec<Plugin>, PluginRepositoryError> {
        Ok(self
            .plugins
            .borrow()
            .iter()
            .filter(|existing| !existing.audit_fields.is_deleted)
            .cloned()
            .collect())
    }

    fn update_state(
        &self,
        plugin_id: &PluginId,
        state: PluginLifecycleState,
        updated_at: i64,
    ) -> Result<bool, PluginRepositoryError> {
        if let Some(plugin) = self
            .plugins
            .borrow_mut()
            .iter_mut()
            .find(|existing| existing.id == *plugin_id && !existing.audit_fields.is_deleted)
        {
            plugin.state = state;
            plugin.audit_fields.updated_at = updated_at;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn soft_delete_plugin(
        &self,
        plugin_id: &PluginId,
        deleted_at: i64,
    ) -> Result<bool, PluginRepositoryError> {
        if let Some(plugin) = self
            .plugins
            .borrow_mut()
            .iter_mut()
            .find(|existing| existing.id == *plugin_id && !existing.audit_fields.is_deleted)
        {
            plugin.audit_fields.updated_at = deleted_at;
            plugin.audit_fields.is_deleted = true;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

struct FakePluginScanner {
    plugins: Vec<DiscoveredPlugin>,
}

impl FakePluginScanner {
    fn new(plugins: Vec<DiscoveredPlugin>) -> Self {
        Self { plugins }
    }
}

impl PluginScanner for Rc<FakePluginScanner> {
    fn scan(&self) -> Result<Vec<DiscoveredPlugin>, PluginScannerError> {
        Ok(self.plugins.clone())
    }
}

struct FixedClock(i64);

impl Clock for FixedClock {
    fn now_timestamp_millis(&self) -> i64 {
        self.0
    }
}
