use std::path::PathBuf;

use ora_contracts::{DiscoveredPlugin, PluginManifest};

use crate::plugin::ports::{PluginScanner, PluginScannerError};

/// On-disk manifest file name expected in each plugin directory.
const MANIFEST_FILE: &str = "plugin.json";

/// Scans a local plugins directory, reading a `plugin.json` manifest from each immediate
/// subdirectory. Implements [`PluginScanner`] for the install/scan flow.
#[derive(Clone, Debug)]
pub struct LocalDirPluginScanner {
    plugins_root: PathBuf,
}

impl LocalDirPluginScanner {
    /// Builds a scanner rooted at the directory holding plugin subdirectories.
    pub fn new(plugins_root: PathBuf) -> Self {
        Self { plugins_root }
    }
}

impl PluginScanner for LocalDirPluginScanner {
    fn scan(&self) -> Result<Vec<DiscoveredPlugin>, PluginScannerError> {
        if !self.plugins_root.exists() {
            return Ok(Vec::new());
        }

        let mut discovered = Vec::new();
        let entries = std::fs::read_dir(&self.plugins_root).map_err(scanner_error_from_io)?;
        for entry in entries {
            let entry = entry.map_err(scanner_error_from_io)?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let manifest_path = path.join(MANIFEST_FILE);
            if !manifest_path.is_file() {
                continue;
            }
            let contents =
                std::fs::read_to_string(&manifest_path).map_err(scanner_error_from_io)?;
            let manifest: PluginManifest =
                serde_json::from_str(&contents).map_err(scanner_error_from_serde)?;
            discovered.push(DiscoveredPlugin {
                source_path: path.to_string_lossy().into_owned(),
                manifest,
            });
        }

        // Deterministic order: stable across scans regardless of filesystem listing order.
        discovered.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        Ok(discovered)
    }
}

fn scanner_error_from_io(error: std::io::Error) -> PluginScannerError {
    PluginScannerError::OperationFailed(error.to_string())
}

fn scanner_error_from_serde(error: serde_json::Error) -> PluginScannerError {
    PluginScannerError::OperationFailed(error.to_string())
}
