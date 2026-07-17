//! Plugin manager facade (placeholder pending the v1 management surface).
//!
//! The original minimal slice — hardcoded plugin id `"1"`, the `number_add` capability, a
//! per-request one-process spawn, and the NDJSON line protocol — has been removed per design-v3
//! §22.4 ("delete the old NDJSON/getNums/returnNums API, no compat layer"). The v1 plugin
//! management surface (manifest scan/identify/validate/install/enablement/registry/state/runtime)
//! is specified in design-v3 §5–§15 and is implemented incrementally under the module layout in
//! §4.3. This stub keeps the crate compiling while that surface is built out; the old
//! `number_add`-based tests were deleted with the slice they exercised.

use crate::config::PluginManagerConfig;

/// Describes the plugin lifecycle state visible to manager callers and tests.
///
/// This is the legacy three-state value; the v1 runtime state machine (§11.1) supersedes it with a
/// closed enum carrying generation/exit/drain detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLifecycleState {
    Registered,
    Running,
    Exited,
}

/// Plugin manager facade.
///
/// Placeholder for the v1 [`PluginManagement`](design-v3 §15.1) trait family; the `Spawner` type
/// parameter is retained so the eventual process-tree spawner (§11.4) can be injected statically.
pub struct PluginManager<Spawner> {
    #[allow(dead_code)]
    config: PluginManagerConfig,
    #[allow(dead_code)]
    process_spawner: Spawner,
}

impl<Spawner> PluginManager<Spawner> {
    /// Builds the plugin manager around a process spawner implementation.
    pub fn new(config: PluginManagerConfig, process_spawner: Spawner) -> Self {
        Self {
            config,
            process_spawner,
        }
    }
}
