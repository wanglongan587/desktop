use crate::document::build_document;
use crate::error::SpecError;
use globset::{Glob, GlobMatcher};
use ora_domain::{SpecDocument, SpecPath, SpecSource};
use ora_logging::ora_debug;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Bounds a single scan so a misconfigured pattern cannot walk an entire drive.
const MAX_SCANNED_ENTRIES: usize = 20_000;

/// Directory names skipped during discovery regardless of the configured patterns.
///
/// These hold generated or vendored trees that never contain authored specs, and walking
/// them dominates scan time in real repositories.
const IGNORED_DIRECTORIES: [&str; 6] = [".git", "node_modules", "target", "dist", ".next", ".venv"];

/// Discovers every spec document under one workspace root.
///
/// Files are matched against each source in configuration order and attributed to the
/// first source that claims them, so overlapping patterns produce one catalog entry
/// rather than duplicates.
pub(crate) fn scan_workspace(
    workspace_root: &Path,
    sources: &[SpecSource],
) -> Result<Vec<SpecDocument>, SpecError> {
    let matchers = compile_matchers(sources)?;
    let mut documents = Vec::new();
    let mut visited = 0usize;

    for relative_path in collect_candidate_files(workspace_root, &mut visited) {
        let normalized = SpecPath::from_relative(&relative_path);
        let Some(source) = matchers
            .iter()
            .find(|(_, matcher)| matcher.is_match(normalized.as_str()))
        else {
            continue;
        };

        let absolute_path = workspace_root.join(&relative_path);
        let content = match std::fs::read_to_string(&absolute_path) {
            Ok(content) => content,
            // A file that disappeared or turned out to be binary is simply not a spec;
            // failing the whole scan would make the catalog hostage to one bad file.
            Err(error) => {
                ora_debug!(path = %absolute_path.display(), error = %error, "skipping unreadable spec candidate");
                continue;
            }
        };
        let file_stem = relative_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();

        documents.push(build_document(
            &source.0.name,
            normalized,
            &file_stem,
            &content,
        ));
    }

    Ok(documents)
}

/// Returns the directories that must be watched to observe changes to the given sources.
///
/// Watching the literal prefix of each pattern keeps the watcher scoped to the few
/// directories that can contain specs, instead of the whole workspace.
pub(crate) fn watch_roots(workspace_root: &Path, sources: &[SpecSource]) -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();

    for source in sources {
        let literal_prefix = source
            .glob
            .split('/')
            .take_while(|segment| !segment.contains(['*', '?', '[', '{']))
            .fold(PathBuf::new(), |path, segment| path.join(segment));
        roots.insert(workspace_root.join(literal_prefix));
    }

    // A watched ancestor already reports its descendants, so nested roots are redundant.
    roots
        .iter()
        .filter(|root| {
            !roots
                .iter()
                .any(|other| other != *root && root.starts_with(other))
        })
        .cloned()
        .collect()
}

/// Compiles each configured pattern once so scanning does not re-parse globs per file.
fn compile_matchers(sources: &[SpecSource]) -> Result<Vec<(SpecSource, GlobMatcher)>, SpecError> {
    sources
        .iter()
        .map(|source| {
            Glob::new(&source.glob)
                .map(|glob| (source.clone(), glob.compile_matcher()))
                .map_err(|error| SpecError::InvalidSourcePattern {
                    pattern: source.glob.clone(),
                    source: error,
                })
        })
        .collect()
}

/// Walks the workspace and yields workspace-relative paths of regular files.
fn collect_candidate_files(workspace_root: &Path, visited: &mut usize) -> Vec<PathBuf> {
    let mut pending = vec![workspace_root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            if *visited >= MAX_SCANNED_ENTRIES {
                ora_debug!(
                    root = %workspace_root.display(),
                    "stopping spec scan at the entry limit"
                );
                return files;
            }
            *visited += 1;

            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                let name = entry.file_name();
                if !IGNORED_DIRECTORIES
                    .iter()
                    .any(|ignored| name.as_os_str() == *ignored)
                {
                    pending.push(path);
                }
                continue;
            }

            if file_type.is_file()
                && let Ok(relative) = path.strip_prefix(workspace_root)
            {
                files.push(relative.to_path_buf());
            }
        }
    }

    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::{scan_workspace, watch_roots};
    use ora_domain::{SpecIdentity, SpecSource};
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Verifies discovery attributes files to sources and skips everything unmatched.
    #[test]
    fn discovers_and_attributes_matching_files() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        write_file(workspace.path(), "docs/specs/design.md", "# Design\n");
        write_file(workspace.path(), "docs/specs/nested/other.md", "# Other\n");
        write_file(workspace.path(), "src/main.rs", "fn main() {}\n");
        write_file(
            workspace.path(),
            "node_modules/pkg/readme.md",
            "# Vendored\n",
        );
        let sources = vec![SpecSource::new("Docs", "docs/specs/**/*.md")];

        let documents = scan_workspace(workspace.path(), &sources)
            .unwrap_or_else(|error| panic!("scan: {error}"));

        assert_eq!(
            documents
                .iter()
                .map(|document| document.path.as_str())
                .collect::<Vec<_>>(),
            vec!["docs/specs/design.md", "docs/specs/nested/other.md"]
        );
        assert_eq!(
            documents
                .iter()
                .map(|document| document.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Design", "Other"]
        );
    }

    /// Verifies the first configured source wins when patterns overlap.
    #[test]
    fn attributes_overlapping_matches_to_the_first_source() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        write_file(workspace.path(), "docs/specs/design.md", "# Design\n");
        let sources = vec![
            SpecSource::new("Primary", "docs/**/*.md"),
            SpecSource::new("Secondary", "docs/specs/*.md"),
        ];

        let documents = scan_workspace(workspace.path(), &sources)
            .unwrap_or_else(|error| panic!("scan: {error}"));

        assert_eq!(
            documents
                .iter()
                .map(|document| document.source_name.as_str())
                .collect::<Vec<_>>(),
            vec!["Primary"]
        );
    }

    /// Verifies a declared identifier is preferred over the path-derived fallback.
    #[test]
    fn honors_declared_identifiers_during_discovery() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        write_file(
            workspace.path(),
            "docs/specs/design.md",
            "---\nid: add-auth\n---\n# Design\n",
        );
        let sources = vec![SpecSource::new("Docs", "docs/specs/**/*.md")];

        let documents = scan_workspace(workspace.path(), &sources)
            .unwrap_or_else(|error| panic!("scan: {error}"));

        assert_eq!(
            documents
                .iter()
                .map(|document| document.identity.clone())
                .collect::<Vec<_>>(),
            vec![SpecIdentity::Declared(ora_domain::SpecId::new("add-auth"))]
        );
    }

    /// Verifies an invalid pattern fails loudly instead of silently matching nothing.
    #[test]
    fn rejects_invalid_patterns() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));

        assert!(
            scan_workspace(
                workspace.path(),
                &[SpecSource::new("Broken", "docs/[unclosed")],
            )
            .is_err()
        );
    }

    /// Verifies watching targets each pattern's literal prefix and drops nested duplicates.
    #[test]
    fn derives_minimal_watch_roots() {
        let workspace = Path::new("/workspace/ora");
        let sources = vec![
            SpecSource::new("Docs", "docs/specs/**/*.md"),
            SpecSource::new("Nested", "docs/specs/nested/*.md"),
            SpecSource::new("OpenSpec", "openspec/changes/**/*.md"),
        ];

        assert_eq!(
            watch_roots(workspace, &sources),
            vec![
                workspace.join("docs").join("specs"),
                workspace.join("openspec").join("changes"),
            ]
        );
    }

    /// Writes one fixture file, creating its parent directories.
    fn write_file(workspace_root: &Path, relative_path: &str, content: &str) {
        let path = relative_path
            .split('/')
            .fold(workspace_root.to_path_buf(), |path, segment| {
                path.join(segment)
            });
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dirs: {error}"));
        }
        fs::write(path, content).unwrap_or_else(|error| panic!("write fixture: {error}"));
    }
}
