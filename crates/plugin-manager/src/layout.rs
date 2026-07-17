//! Disk layout for the plugin subsystem (design-v3 §6.1).
//!
//! All paths derive from a single `<ORA_DATA_DIR>` root, kept distinct so code paths cannot
//! accidentally cross (§6.1: `plugins/` is code, `plugin-data/` is mutable data, never the same;
//! candidate paths can never become a runtime cwd/entry). This module only computes paths; the
//! safe no-follow filesystem operations (§14.2) are a separate, Windows-FFI-gated piece.

use std::path::{Path, PathBuf};

use ora_plugin_protocol::{ContentOwnerId, PluginId};

/// The plugin-subsystem disk layout rooted at `<ORA_DATA_DIR>` (§6.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLayout {
    data_dir: PathBuf,
}

impl PluginLayout {
    /// Builds the layout rooted at the given Ora data directory.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
        }
    }

    /// Returns the data directory root.
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// `plugin-runtime/<runtime-version>/` (§6.1): pinned Bun, bootstrap, empty bunfig, receipt.
    pub fn runtime_dir(&self, runtime_version: &str) -> PathBuf {
        self.data_dir.join("plugin-runtime").join(runtime_version)
    }

    /// `plugins/` — the managed code root (immutable package files + `.ora/receipt.json`).
    pub fn plugins_root(&self) -> PathBuf {
        self.data_dir.join("plugins")
    }

    /// `plugins/.staging/` — same-volume staging for installs (§6.1, §9.1).
    pub fn staging_root(&self) -> PathBuf {
        self.plugins_root().join(".staging")
    }

    /// `plugins/.staging/<operation-id>/` (§6.1).
    pub fn staging_dir(&self, operation_id: &str) -> PathBuf {
        self.staging_root().join(operation_id)
    }

    /// `plugins/.trash/` — recoverable tombstone/trash root (§6.1, §10.3).
    pub fn trash_root(&self) -> PathBuf {
        self.plugins_root().join(".trash")
    }

    /// `plugins/.trash/<operation-id>/` (§6.1).
    pub fn trash_dir(&self, operation_id: &str) -> PathBuf {
        self.trash_root().join(operation_id)
    }

    /// `plugins/<canonical-plugin-id>/` — a managed installed plugin's code root (§6.1).
    pub fn plugin_dir(&self, plugin_id: &PluginId) -> PathBuf {
        self.plugins_root().join(plugin_id.as_str())
    }

    /// `plugins/<id>/.ora/receipt.json` — the Host-generated install receipt (§6.2).
    pub fn receipt_path(&self, plugin_id: &PluginId) -> PathBuf {
        self.plugin_dir(plugin_id).join(".ora").join("receipt.json")
    }

    /// `plugin-system/` — Host state root (§6.1).
    pub fn plugin_system_dir(&self) -> PathBuf {
        self.data_dir.join("plugin-system")
    }

    /// `plugin-system/state.json` — the single-writer Host state (§6.4).
    pub fn state_path(&self) -> PathBuf {
        self.plugin_system_dir().join("state.json")
    }

    /// `plugin-system/state.previous.json` — the backup (§6.4).
    pub fn state_previous_path(&self) -> PathBuf {
        self.plugin_system_dir().join("state.previous.json")
    }

    /// `plugin-system/manager.lock` — the process-lifetime lease handle (§6.1).
    pub fn manager_lock_path(&self) -> PathBuf {
        self.plugin_system_dir().join("manager.lock")
    }

    /// `plugin-data/` — mutable per-plugin data root (distinct from `plugins/`, §6.1).
    pub fn plugin_data_root(&self) -> PathBuf {
        self.data_dir.join("plugin-data")
    }

    /// `plugin-data/<canonical-plugin-id>/` (§6.1).
    pub fn plugin_data_dir(&self, plugin_id: &PluginId) -> PathBuf {
        self.plugin_data_root().join(plugin_id.as_str())
    }

    /// `plugin-data/<id>/<content-owner-id>/` — the content-owner-scoped mutable storage (§6.1).
    pub fn storage_path(&self, plugin_id: &PluginId, content_owner: &ContentOwnerId) -> PathBuf {
        self.plugin_data_dir(plugin_id).join(content_owner.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn layout() -> PluginLayout {
        PluginLayout::new(PathBuf::from("/ora-data"))
    }

    fn pid() -> PluginId {
        PluginId::try_new("ora.claude-code".to_string()).unwrap_or_else(|e| panic!("pid: {e}"))
    }

    fn owner() -> ContentOwnerId {
        ContentOwnerId::try_new(format!("sha256-{}", "a".repeat(64)))
            .unwrap_or_else(|e| panic!("owner: {e}"))
    }

    #[test]
    fn runtime_and_system_paths_derive_from_data_dir() {
        let layout = layout();
        assert_eq!(
            layout.runtime_dir("0.1.0"),
            PathBuf::from("/ora-data/plugin-runtime/0.1.0")
        );
        assert_eq!(
            layout.state_path(),
            PathBuf::from("/ora-data/plugin-system/state.json")
        );
        assert_eq!(
            layout.state_previous_path(),
            PathBuf::from("/ora-data/plugin-system/state.previous.json")
        );
        assert_eq!(
            layout.manager_lock_path(),
            PathBuf::from("/ora-data/plugin-system/manager.lock")
        );
    }

    #[test]
    fn plugin_code_and_data_roots_are_distinct() {
        let layout = layout();
        // plugins/ (code) != plugin-data/ (mutable data).
        assert_ne!(layout.plugins_root(), layout.plugin_data_root());
        assert_eq!(
            layout.plugin_dir(&pid()),
            PathBuf::from("/ora-data/plugins/ora.claude-code")
        );
        assert_eq!(
            layout.plugin_data_dir(&pid()),
            PathBuf::from("/ora-data/plugin-data/ora.claude-code")
        );
        assert_eq!(
            layout.storage_path(&pid(), &owner()),
            PathBuf::from(format!(
                "/ora-data/plugin-data/ora.claude-code/sha256-{}",
                "a".repeat(64)
            ))
        );
    }

    #[test]
    fn staging_and_trash_are_under_plugins_root() {
        let layout = layout();
        assert_eq!(
            layout.staging_root(),
            PathBuf::from("/ora-data/plugins/.staging")
        );
        assert_eq!(
            layout.staging_dir("op-1"),
            PathBuf::from("/ora-data/plugins/.staging/op-1")
        );
        assert_eq!(
            layout.trash_root(),
            PathBuf::from("/ora-data/plugins/.trash")
        );
        assert_eq!(
            layout.trash_dir("op-2"),
            PathBuf::from("/ora-data/plugins/.trash/op-2")
        );
        assert_eq!(
            layout.receipt_path(&pid()),
            PathBuf::from("/ora-data/plugins/ora.claude-code/.ora/receipt.json")
        );
    }
}
