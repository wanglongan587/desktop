use crate::error::SpecError;
use crate::scanner::{scan_workspace, watch_roots};
use crate::source::load_spec_sources;
use crate::watcher::{ChangeSignal, SpecWatcher};
use ora_domain::{SpecDocument, SpecSource};
use ora_logging::ora_debug;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Captures one consistent view of a workspace's spec catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecSnapshot {
    pub workspace_root: PathBuf,
    pub sources: Vec<SpecSource>,
    pub documents: Vec<SpecDocument>,
}

/// Holds the indexed state of the workspace currently being viewed.
struct ActiveWorkspace {
    snapshot: SpecSnapshot,
    watcher: SpecWatcher,
    signal: Arc<ChangeSignal>,
    scanned_generation: u64,
}

/// Serves spec catalogs for the workspace the user is currently looking at.
///
/// Only one workspace is indexed at a time. A task backed by a linked worktree is a
/// different branch with a different set of spec files, and the user views exactly one of
/// them; keeping every visited workspace warm would accumulate operating system watch
/// handles for branches nobody is reading. Switching workspaces therefore releases the
/// previous watcher and indexes the new root.
pub struct SpecCatalog {
    extra_sources: Vec<SpecSource>,
    active: Mutex<Option<ActiveWorkspace>>,
}

impl SpecCatalog {
    /// Builds a catalog whose per-user source additions apply to every workspace.
    pub fn new(extra_sources: Vec<SpecSource>) -> Self {
        Self {
            extra_sources,
            active: Mutex::new(None),
        }
    }

    /// Returns the current catalog for one workspace, indexing it on first use.
    ///
    /// A cached snapshot is reused only while the watcher reports no change, so repeated
    /// polling costs nothing when nothing on disk moved.
    pub fn snapshot(&self, workspace_root: &Path) -> Result<SpecSnapshot, SpecError> {
        let workspace_root = canonical_root(workspace_root)?;
        let mut active = self.lock_active();

        if let Some(current) = active.as_ref()
            && current.snapshot.workspace_root == workspace_root
            && current.signal.generation() == current.scanned_generation
        {
            return Ok(current.snapshot.clone());
        }

        let replacement = match active.take() {
            // Reindexing the same workspace only needs a rescan; the watcher and its OS
            // handles remain valid, and re-registering them would drop events.
            Some(current) if current.snapshot.workspace_root == workspace_root => {
                self.rescan(current)?
            }
            _ => self.index(workspace_root)?,
        };
        let snapshot = replacement.snapshot.clone();
        *active = Some(replacement);

        Ok(snapshot)
    }

    /// Reads one catalogued document together with its raw markdown body.
    ///
    /// The path is resolved against the catalog rather than against the filesystem, so a
    /// request for a file outside the configured sources is rejected before any read.
    pub fn read_document(
        &self,
        workspace_root: &Path,
        relative_path: &str,
    ) -> Result<(SpecDocument, String), SpecError> {
        let snapshot = self.snapshot(workspace_root)?;
        let document = snapshot
            .documents
            .iter()
            .find(|document| document.path.as_str() == relative_path)
            .ok_or_else(|| SpecError::DocumentNotFound {
                path: relative_path.to_string(),
            })?
            .clone();

        let absolute_path = document
            .path
            .as_str()
            .split('/')
            .fold(snapshot.workspace_root, |path, segment| path.join(segment));
        let content = std::fs::read_to_string(&absolute_path).map_err(|source| {
            ora_debug!(path = %absolute_path.display(), error = %source, "failed to read spec document");
            SpecError::DocumentNotFound {
                path: relative_path.to_string(),
            }
        })?;

        Ok((document, content))
    }

    /// Indexes a workspace from scratch and starts watching its spec directories.
    fn index(&self, workspace_root: PathBuf) -> Result<ActiveWorkspace, SpecError> {
        let sources = load_spec_sources(&workspace_root, &self.extra_sources)?;
        let watcher = SpecWatcher::start(&workspace_root, &watch_roots(&workspace_root, &sources))?;
        let signal = watcher.signal();
        // Reading the generation before scanning means a change racing with this scan is
        // observed as pending rather than silently swallowed.
        let scanned_generation = signal.generation();
        let documents = scan_workspace(&workspace_root, &sources)?;

        Ok(ActiveWorkspace {
            snapshot: SpecSnapshot {
                workspace_root,
                sources,
                documents,
            },
            watcher,
            signal,
            scanned_generation,
        })
    }

    /// Refreshes an already-watched workspace without restarting its watcher.
    fn rescan(&self, current: ActiveWorkspace) -> Result<ActiveWorkspace, SpecError> {
        let ActiveWorkspace {
            snapshot,
            watcher,
            signal,
            scanned_generation: _,
        } = current;
        let workspace_root = snapshot.workspace_root;
        // Sources are reloaded too, because `.ora/specs.toml` lives inside the workspace
        // and editing it must take effect without restarting Ora.
        let sources = load_spec_sources(&workspace_root, &self.extra_sources)?;
        let scanned_generation = signal.generation();
        let documents = scan_workspace(&workspace_root, &sources)?;

        Ok(ActiveWorkspace {
            snapshot: SpecSnapshot {
                workspace_root,
                sources,
                documents,
            },
            watcher,
            signal,
            scanned_generation,
        })
    }

    /// Recovers the active slot after a panic elsewhere rather than propagating poison.
    ///
    /// The catalog holds only rebuildable cache state, so a poisoned lock is safe to reuse.
    fn lock_active(&self) -> std::sync::MutexGuard<'_, Option<ActiveWorkspace>> {
        match self.active.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Resolves the workspace root and rejects one that is missing or not a directory.
///
/// Canonicalizing matters beyond tidiness: the catalog compares roots to decide whether a
/// workspace switch occurred, and the same directory can otherwise arrive spelled several
/// different ways.
fn canonical_root(workspace_root: &Path) -> Result<PathBuf, SpecError> {
    let canonical =
        workspace_root
            .canonicalize()
            .map_err(|source| SpecError::WorkspaceUnavailable {
                path: workspace_root.to_path_buf(),
                source,
            })?;

    if !canonical.is_dir() {
        return Err(SpecError::WorkspaceUnavailable {
            path: workspace_root.to_path_buf(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "spec workspace must be a directory",
            ),
        });
    }

    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::SpecCatalog;
    use ora_domain::SpecSource;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    /// Verifies the catalog exposes discovered documents alongside the resolved sources.
    #[test]
    fn serves_discovered_documents() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        write_file(workspace.path(), "docs/specs/design.md", "# Design\n");
        let catalog = SpecCatalog::new(Vec::new());

        let snapshot = catalog
            .snapshot(workspace.path())
            .unwrap_or_else(|error| panic!("snapshot: {error}"));

        assert_eq!(
            snapshot
                .documents
                .iter()
                .map(|document| document.path.as_str())
                .collect::<Vec<_>>(),
            vec!["docs/specs/design.md"]
        );
        assert_eq!(snapshot.sources, crate::source::default_spec_sources());
    }

    /// Verifies rewriting a file with identical bytes leaves the catalog unchanged.
    ///
    /// This is the behavior that timestamp-based freshness gets wrong: editors with
    /// autosave and format-on-save rewrite unchanged content constantly, and reacting to
    /// those writes would churn the catalog for no reason.
    #[test]
    fn ignores_rewrites_that_do_not_change_content() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let spec_path = write_file(workspace.path(), "docs/specs/design.md", "# Design\n");
        let catalog = SpecCatalog::new(Vec::new());
        let before = catalog
            .snapshot(workspace.path())
            .unwrap_or_else(|error| panic!("snapshot: {error}"));

        sleep(Duration::from_millis(20));
        fs::write(&spec_path, "# Design\n").unwrap_or_else(|error| panic!("rewrite: {error}"));
        sleep(Duration::from_millis(600));

        assert_eq!(
            catalog
                .snapshot(workspace.path())
                .unwrap_or_else(|error| panic!("snapshot: {error}")),
            before
        );
    }

    /// Verifies a genuine edit is observed without an explicit refresh request.
    #[test]
    fn observes_content_changes() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let spec_path = write_file(workspace.path(), "docs/specs/design.md", "# Design\n");
        let catalog = SpecCatalog::new(Vec::new());
        let before = catalog
            .snapshot(workspace.path())
            .unwrap_or_else(|error| panic!("snapshot: {error}"));

        fs::write(&spec_path, "# Rewritten design\n")
            .unwrap_or_else(|error| panic!("rewrite: {error}"));

        assert_eq!(
            wait_for_title(&catalog, workspace.path(), "Rewritten design"),
            true,
            "expected the catalog to observe the edited title, last snapshot was {before:?}"
        );
    }

    /// Verifies reads are restricted to documents the catalog actually contains.
    #[test]
    fn rejects_reads_outside_the_catalog() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        write_file(workspace.path(), "docs/specs/design.md", "# Design\n");
        write_file(workspace.path(), "secrets.env", "TOKEN=1\n");
        let catalog = SpecCatalog::new(Vec::new());

        let (document, content) = catalog
            .read_document(workspace.path(), "docs/specs/design.md")
            .unwrap_or_else(|error| panic!("read: {error}"));

        assert_eq!(content, "# Design\n".to_string());
        assert_eq!(document.title, "Design".to_string());
        assert!(
            catalog
                .read_document(workspace.path(), "secrets.env")
                .is_err()
        );
        assert!(
            catalog
                .read_document(workspace.path(), "../outside.md")
                .is_err()
        );
    }

    /// Verifies per-user additions extend the presets for every workspace.
    #[test]
    fn applies_extra_sources() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        write_file(workspace.path(), "scratch/idea.md", "# Idea\n");
        let catalog = SpecCatalog::new(vec![SpecSource::new("Scratch", "scratch/*.md")]);

        let snapshot = catalog
            .snapshot(workspace.path())
            .unwrap_or_else(|error| panic!("snapshot: {error}"));

        assert_eq!(
            snapshot
                .documents
                .iter()
                .map(|document| document.source_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Scratch"]
        );
    }

    /// Polls the catalog until the expected title appears or a bounded deadline elapses.
    fn wait_for_title(catalog: &SpecCatalog, workspace_root: &Path, expected: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(6);

        while Instant::now() < deadline {
            let snapshot = catalog
                .snapshot(workspace_root)
                .unwrap_or_else(|error| panic!("snapshot: {error}"));
            if snapshot
                .documents
                .iter()
                .any(|document| document.title == expected)
            {
                return true;
            }
            sleep(Duration::from_millis(50));
        }

        false
    }

    /// Writes one fixture file and returns its absolute path.
    fn write_file(workspace_root: &Path, relative_path: &str, content: &str) -> std::path::PathBuf {
        let path = relative_path
            .split('/')
            .fold(workspace_root.to_path_buf(), |path, segment| {
                path.join(segment)
            });
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dirs: {error}"));
        }
        fs::write(&path, content).unwrap_or_else(|error| panic!("write fixture: {error}"));

        path
    }
}
