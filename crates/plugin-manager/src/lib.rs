mod candidate_authority;
mod catalog;
mod config;
mod discovery;
mod enablement;
mod error;
mod events;
mod install;
mod issue;
mod lease;
mod manifest;
mod package_validation;
mod persistence;
mod ports;
mod registry;
mod runtime;
mod runtime_events;
mod safe_fs;
mod scanner;
mod service;
mod state;
mod validation;

pub use candidate_authority::*;
pub use catalog::*;
pub use config::*;
pub use enablement::*;
pub use error::*;
pub use events::*;
pub use install::*;
pub use issue::{PluginDiscoveryIssue, PluginDiscoveryIssueKind};
pub use lease::*;
pub use package_validation::*;
pub use persistence::*;
pub use ports::*;
pub use registry::*;
pub use runtime::*;
pub use runtime_events::*;
pub use safe_fs::*;
pub use scanner::*;
pub use service::*;
pub use state::*;
pub use validation::{
    InstalledPlugin as DiscoveredPlugin, InstalledPluginAgent as DiscoveredPluginAgent,
    PluginEngines as DiscoveredPluginEngines, PluginKind as DiscoveredPluginKind,
    PluginPackageType as DiscoveredPluginPackageType,
};

use std::path::Path;

/// The maximum number of bytes read from one plugin package manifest.
pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

/// Owns one immutable startup snapshot of installed plugins and discovery problems.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManager {
    installed_plugins: Vec<DiscoveredPlugin>,
    discovery_issues: Vec<PluginDiscoveryIssue>,
}

impl PluginManager {
    /// Discovers direct child plugin packages below `<data_dir>/plugins` without executing them.
    pub fn discover(data_dir: impl AsRef<Path>) -> Self {
        let discovery::PluginDiscovery {
            installed_plugins,
            discovery_issues,
        } = discovery::discover(data_dir.as_ref());

        Self {
            installed_plugins,
            discovery_issues,
        }
    }

    /// Returns the valid installed plugins in stable identifier order.
    pub fn installed_plugins(&self) -> &[DiscoveredPlugin] {
        &self.installed_plugins
    }

    /// Returns non-fatal problems encountered while building the snapshot.
    pub fn discovery_issues(&self) -> &[PluginDiscoveryIssue] {
        &self.discovery_issues
    }
}
