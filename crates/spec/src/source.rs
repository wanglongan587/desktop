use crate::error::SpecError;
use ora_domain::SpecSource;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Locates the repository-owned source configuration relative to a workspace root.
pub const SPEC_SOURCE_CONFIG_RELATIVE_PATH: [&str; 2] = [".ora", "specs.toml"];

/// Mirrors the on-disk shape of `.ora/specs.toml`.
#[derive(Debug, Deserialize)]
struct SourceConfigFile {
    #[serde(default)]
    source: Vec<SourceConfigEntry>,
}

/// Mirrors one `[[source]]` table in the configuration file.
#[derive(Debug, Deserialize)]
struct SourceConfigEntry {
    name: String,
    glob: String,
}

/// Returns the sources assumed when a workspace declares none of its own.
///
/// The presets cover the tools Ora already drives, so a repository that adopted OpenSpec
/// or the superpowers workflow shows its existing documents without any configuration.
pub fn default_spec_sources() -> Vec<SpecSource> {
    vec![
        SpecSource::new("OpenSpec", "openspec/changes/**/*.md"),
        SpecSource::new("Superpowers", "docs/superpowers/specs/**/*.md"),
        SpecSource::new("Docs", "docs/specs/**/*.md"),
    ]
}

/// Resolves the discovery sources that apply to one workspace.
///
/// A configuration file replaces the presets rather than extending them: a team that
/// describes its own layout is stating where specs live, and silently scanning extra
/// directories would contradict that. `extra_sources` carries per-user additions that are
/// appended after whichever base was selected.
pub fn load_spec_sources(
    workspace_root: &Path,
    extra_sources: &[SpecSource],
) -> Result<Vec<SpecSource>, SpecError> {
    let config_path = SPEC_SOURCE_CONFIG_RELATIVE_PATH
        .iter()
        .fold(workspace_root.to_path_buf(), |path, segment| {
            path.join(segment)
        });

    let mut sources = match read_config_file(&config_path)? {
        Some(configured) => configured,
        None => default_spec_sources(),
    };
    sources.extend_from_slice(extra_sources);

    Ok(sources)
}

/// Reads the optional configuration file, distinguishing "absent" from "unreadable".
fn read_config_file(config_path: &PathBuf) -> Result<Option<Vec<SpecSource>>, SpecError> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(SpecError::WorkspaceUnavailable {
                path: config_path.clone(),
                source,
            });
        }
    };

    let parsed: SourceConfigFile =
        toml::from_str(&content).map_err(|source| SpecError::InvalidSourceConfiguration {
            path: config_path.clone(),
            source,
        })?;

    Ok(Some(
        parsed
            .source
            .into_iter()
            .map(|entry| SpecSource::new(entry.name, entry.glob))
            .collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{default_spec_sources, load_spec_sources};
    use ora_domain::SpecSource;
    use pretty_assertions::assert_eq;
    use std::fs;
    use tempfile::TempDir;

    /// Verifies a workspace without configuration falls back to the built-in presets.
    #[test]
    fn falls_back_to_presets_without_configuration() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));

        assert_eq!(
            load_spec_sources(workspace.path(), &[]).unwrap_or_else(|error| panic!("{error}")),
            default_spec_sources()
        );
    }

    /// Verifies a configured workspace replaces the presets and still accepts personal additions.
    #[test]
    fn replaces_presets_with_configuration_and_appends_extras() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let config_directory = workspace.path().join(".ora");
        fs::create_dir_all(&config_directory)
            .unwrap_or_else(|error| panic!("create config dir: {error}"));
        fs::write(
            config_directory.join("specs.toml"),
            "[[source]]\nname = \"Design\"\nglob = \"design/**/*.md\"\n",
        )
        .unwrap_or_else(|error| panic!("write config: {error}"));
        let extra = SpecSource::new("Scratch", "scratch/*.md");

        assert_eq!(
            load_spec_sources(workspace.path(), std::slice::from_ref(&extra))
                .unwrap_or_else(|error| panic!("{error}")),
            vec![SpecSource::new("Design", "design/**/*.md"), extra]
        );
    }

    /// Verifies malformed configuration surfaces as an error instead of silently degrading.
    #[test]
    fn rejects_malformed_configuration() {
        let workspace = TempDir::new().unwrap_or_else(|error| panic!("temp dir: {error}"));
        let config_directory = workspace.path().join(".ora");
        fs::create_dir_all(&config_directory)
            .unwrap_or_else(|error| panic!("create config dir: {error}"));
        fs::write(
            config_directory.join("specs.toml"),
            "[[source]]\nname = 1\n",
        )
        .unwrap_or_else(|error| panic!("write config: {error}"));

        assert!(load_spec_sources(workspace.path(), &[]).is_err());
    }
}
