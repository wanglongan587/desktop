use crate::error::SpecError;
use crate::scanner::WatchTarget;
use notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use ora_logging::ora_debug;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Collapses editor write bursts into a single rescan.
///
/// Editors commonly save by writing a temporary file and renaming it, and formatters may
/// rewrite immediately afterwards, so raw events arrive in clusters rather than singly.
const DEBOUNCE_INTERVAL: Duration = Duration::from_millis(300);

/// Signals that watched spec directories changed since a given generation.
///
/// Only a counter is published rather than the events themselves: the catalog reacts by
/// rescanning, and a rescan is what correctly handles creations, deletions and the
/// delete/create pair that a rename produces.
#[derive(Debug, Default)]
pub(crate) struct ChangeSignal {
    generation: AtomicU64,
}

impl ChangeSignal {
    /// Returns the generation observed by the most recent rescan.
    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Records that watched directories changed.
    fn mark_changed(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

/// Keeps watched spec directories under observation for the lifetime of one workspace.
///
/// Dropping the watcher stops observation, which is how the catalog releases operating
/// system handles when the active workspace changes.
pub(crate) struct SpecWatcher {
    signal: Arc<ChangeSignal>,
    // Held to keep the debouncer thread and its OS watch handles alive.
    _debouncer: Debouncer<notify::RecommendedWatcher, RecommendedCache>,
}

impl SpecWatcher {
    /// Starts watching every existing target, ignoring directories that do not exist yet.
    ///
    /// Absent roots are expected rather than exceptional: a workspace may have adopted
    /// only one of the configured spec tools. The workspace root is watched
    /// non-recursively as a fallback so that later creation of a missing root-level
    /// file (or directory) is still observed without walking the whole tree.
    pub(crate) fn start(workspace_root: &Path, targets: &[WatchTarget]) -> Result<Self, SpecError> {
        let signal = Arc::new(ChangeSignal::default());
        let notify_signal = Arc::clone(&signal);
        let mut debouncer = new_debouncer(
            DEBOUNCE_INTERVAL,
            /*tick_rate*/ None,
            move |result: DebounceEventResult| match result {
                Ok(events) if events.is_empty() => {}
                Ok(_) => notify_signal.mark_changed(),
                // A dropped or failed watch leaves the index potentially stale, so the
                // conservative response is to force a rescan rather than trust the cache.
                Err(errors) => {
                    ora_debug!(error_count = errors.len(), "spec watcher reported errors");
                    notify_signal.mark_changed();
                }
            },
        )
        .map_err(|source| SpecError::WatchFailed {
            path: workspace_root.to_path_buf(),
            source,
        })?;

        let mut watched_any = false;
        for target in targets {
            let path = target.path();
            if !path.is_dir() {
                continue;
            }
            let mode = match target {
                WatchTarget::Recursive(_) => RecursiveMode::Recursive,
                WatchTarget::NonRecursive(_) => RecursiveMode::NonRecursive,
            };
            if debouncer.watch(path, mode).is_ok() {
                watched_any = true;
            }
        }

        if !watched_any {
            debouncer
                .watch(workspace_root, RecursiveMode::NonRecursive)
                .map_err(|source| SpecError::WatchFailed {
                    path: workspace_root.to_path_buf(),
                    source,
                })?;
        }

        Ok(Self {
            signal,
            _debouncer: debouncer,
        })
    }

    /// Returns the shared signal the catalog polls before serving a snapshot.
    pub(crate) fn signal(&self) -> Arc<ChangeSignal> {
        Arc::clone(&self.signal)
    }
}

#[cfg(test)]
mod tests {
    use super::{DEBOUNCE_INTERVAL, SpecWatcher};
    use crate::scanner::WatchTarget;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    /// Verifies a write inside a watched root advances the change generation.
    #[test]
    fn reports_changes_under_watched_roots() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let watched = workspace.path().join("docs").join("specs");
        fs::create_dir_all(&watched).unwrap_or_else(|error| panic!("create dirs: {error}"));
        let target = WatchTarget::Recursive(watched.clone());
        let watcher = SpecWatcher::start(workspace.path(), std::slice::from_ref(&target))
            .unwrap_or_else(|error| panic!("start watcher: {error}"));
        let signal = watcher.signal();
        let before = signal.generation();

        fs::write(watched.join("design.md"), "# Design\n")
            .unwrap_or_else(|error| panic!("write spec: {error}"));

        assert_eq!(wait_for_change(&signal, before), true);
    }

    /// Verifies a workspace whose configured roots are absent still starts watching.
    #[test]
    fn falls_back_to_the_workspace_root() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let missing = WatchTarget::Recursive(workspace.path().join("openspec").join("changes"));

        assert!(SpecWatcher::start(workspace.path(), &[missing]).is_ok());
    }

    /// Verifies a non-recursive root watch observes a root-level SPEC.md write.
    #[test]
    fn reports_root_level_spec_writes() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let target = WatchTarget::NonRecursive(workspace.path().to_path_buf());
        let watcher = SpecWatcher::start(workspace.path(), std::slice::from_ref(&target))
            .unwrap_or_else(|error| panic!("start watcher: {error}"));
        let signal = watcher.signal();
        let before = signal.generation();

        fs::write(workspace.path().join("SPEC.md"), "# Brief\n")
            .unwrap_or_else(|error| panic!("write spec: {error}"));

        assert_eq!(wait_for_change(&signal, before), true);
    }

    /// Blocks until the generation advances or a bounded deadline elapses.
    fn wait_for_change(signal: &super::ChangeSignal, before: u64) -> bool {
        let deadline = Instant::now() + DEBOUNCE_INTERVAL * 20;

        while Instant::now() < deadline {
            if signal.generation() != before {
                return true;
            }
            sleep(Duration::from_millis(25));
        }

        false
    }
}
