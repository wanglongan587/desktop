use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

/// Generates artifacts in an isolated workspace and compares them with tracked output.
pub(crate) fn check_generated_paths<Generate>(
    workspace_root: &Path,
    relative_paths: &[&Path],
    generate: Generate,
) -> Result<(), Box<dyn Error>>
where
    Generate: FnOnce(&Path) -> Result<(), Box<dyn Error>>,
{
    let isolated_workspace = tempfile::tempdir()?;
    generate(isolated_workspace.path())?;

    let mut drift = Vec::new();
    for relative_path in relative_paths {
        compare_tree(
            &workspace_root.join(relative_path),
            &isolated_workspace.path().join(relative_path),
            relative_path,
            &mut drift,
        )?;
    }

    if drift.is_empty() {
        return Ok(());
    }

    Err(format!(
        "{}; run the matching export task and commit its output",
        drift.join(", ")
    )
    .into())
}

/// Compares one generated directory recursively so missing and unexpected files are visible.
fn compare_tree(
    tracked_root: &Path,
    generated_root: &Path,
    display_root: &Path,
    drift: &mut Vec<String>,
) -> Result<(), Box<dyn Error>> {
    let tracked_files = collect_relative_files(tracked_root)?;
    let generated_files = collect_relative_files(generated_root)?;

    for relative_file in tracked_files.union(&generated_files) {
        let display_path = display_root.join(relative_file);
        let tracked_path = tracked_root.join(relative_file);
        let generated_path = generated_root.join(relative_file);
        match (tracked_path.exists(), generated_path.exists()) {
            (true, false) => drift.push(format!(
                "unexpected tracked file {}",
                display_path.display()
            )),
            (false, true) => drift.push(format!("missing tracked file {}", display_path.display())),
            (true, true) if fs::read(&tracked_path)? != fs::read(&generated_path)? => {
                drift.push(format!("outdated file {}", display_path.display()));
            }
            (true, true) => {}
            (false, false) => unreachable!("union entries must exist in at least one tree"),
        }
    }

    Ok(())
}

/// Collects deterministic file paths without depending on platform path separators.
fn collect_relative_files(root: &Path) -> Result<BTreeSet<PathBuf>, Box<dyn Error>> {
    if !root.exists() {
        return Ok(BTreeSet::new());
    }

    let mut files = BTreeSet::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative_directory) = pending.pop() {
        let directory = root.join(&relative_directory);
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let relative_path = relative_directory.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                pending.push(relative_path);
            } else {
                files.insert(relative_path);
            }
        }
    }

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::check_generated_paths;
    use pretty_assertions::assert_eq;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    /// Proves a matching tree passes without mutating the tracked workspace.
    #[test]
    fn accepts_matching_generated_tree_without_writes() {
        let workspace = test_workspace("expected");

        check_generated_paths(workspace.path(), &[Path::new("generated")], |isolated| {
            write_fixture(isolated, "expected")
        })
        .unwrap_or_else(|error| panic!("expected matching generation: {error}"));

        assert_eq!(
            fs::read_to_string(workspace.path().join("generated/output.txt"))
                .unwrap_or_else(|error| panic!("expected tracked output: {error}")),
            "expected"
        );
    }

    /// Proves drift reports the affected path while leaving tracked bytes untouched.
    #[test]
    fn rejects_outdated_generated_tree_without_writes() {
        let workspace = test_workspace("stale");

        let error =
            check_generated_paths(workspace.path(), &[Path::new("generated")], |isolated| {
                write_fixture(isolated, "expected")
            })
            .expect_err("stale output must fail");

        let expected_error = format!(
            "outdated file {}; run the matching export task and commit its output",
            Path::new("generated").join("output.txt").display()
        );
        assert_eq!(
            (
                error.to_string(),
                fs::read_to_string(workspace.path().join("generated/output.txt"))
                    .unwrap_or_else(|read_error| panic!("expected tracked output: {read_error}")),
            ),
            (expected_error, "stale".to_owned())
        );
    }

    fn test_workspace(contents: &str) -> TempDir {
        let workspace =
            TempDir::new().unwrap_or_else(|error| panic!("expected temporary workspace: {error}"));
        write_fixture(workspace.path(), contents)
            .unwrap_or_else(|error| panic!("expected fixture output: {error}"));
        workspace
    }

    /// Writes the minimal generated tree used to exercise byte-for-byte comparison.
    fn write_fixture(root: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
        let generated = root.join("generated");
        fs::create_dir_all(&generated)?;
        fs::write(generated.join("output.txt"), contents)?;
        Ok(())
    }
}
