use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::path::Path;

/// Identifies one spec document across scans.
///
/// This is deliberately not a persisted database key: spec documents live in the
/// repository, not in Ora's storage, so their identity is either declared inside the
/// document or derived from its location.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpecId(String);

impl SpecId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for SpecId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Display for SpecId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Records how a spec document obtained its identity.
///
/// The distinction matters beyond presentation: only a declared identity survives a
/// rename, because filesystem watchers report renames as an unrelated delete/create
/// pair. Provenance links may therefore only be anchored to declared identities, so
/// callers are forced to handle the derived case explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpecIdentity {
    /// The document declares its own identifier in frontmatter.
    Declared(SpecId),
    /// The document has no declared identifier, so its path stands in as identity.
    Derived(SpecId),
}

impl SpecIdentity {
    /// Returns the effective identifier regardless of how it was obtained.
    pub fn id(&self) -> &SpecId {
        match self {
            SpecIdentity::Declared(id) | SpecIdentity::Derived(id) => id,
        }
    }
}

/// Locates one spec document relative to the workspace root that contains it.
///
/// The stored form always uses forward slashes. Normalizing is deliberate rather than
/// cosmetic: a document without a declared identifier is identified by its path, and
/// that identity must stay stable when the same repository is opened on Windows and on
/// Unix.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SpecPath(String);

impl SpecPath {
    /// Builds a workspace-relative spec path with platform-independent separators.
    pub fn from_relative(relative_path: &Path) -> Self {
        let normalized = relative_path
            .components()
            .map(|component| component.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");

        Self(normalized)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SpecPath {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Fingerprints the bytes of one spec document.
///
/// Freshness decisions compare this value rather than modification timestamps, because
/// editors, auto-save and formatters routinely rewrite a file with identical content
/// and would otherwise produce a stream of false changes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SpecContentHash(String);

impl SpecContentHash {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SpecContentHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Declares one discovery rule that maps a glob pattern to a named group of specs.
///
/// Sources are what let Ora present specs produced by unrelated tooling (OpenSpec,
/// superpowers, hand-written documents) as a single catalog without teaching Ora each
/// tool's internal conventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecSource {
    pub name: String,
    pub glob: String,
}

impl SpecSource {
    pub fn new(name: impl Into<String>, glob: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            glob: glob.into(),
        }
    }
}

/// Describes one spec document discovered inside a workspace.
///
/// Instances are always derived from the filesystem and never persisted: the index can
/// be rebuilt in full from disk at any time, so the document carries no audit metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecDocument {
    pub identity: SpecIdentity,
    pub source_name: String,
    pub path: SpecPath,
    pub title: String,
    pub content_hash: SpecContentHash,
}

impl SpecDocument {
    pub fn new(
        identity: SpecIdentity,
        source_name: impl Into<String>,
        path: SpecPath,
        title: impl Into<String>,
        content_hash: SpecContentHash,
    ) -> Self {
        Self {
            identity,
            source_name: source_name.into(),
            path,
            title: title.into(),
            content_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SpecContentHash, SpecDocument, SpecId, SpecIdentity, SpecPath, SpecSource};
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

    /// Verifies nested relative paths collapse to a single platform-independent form.
    ///
    /// The path is assembled from components so the source of the separator is the
    /// running platform, which is exactly what normalization has to erase.
    #[test]
    fn normalizes_relative_paths_to_forward_slashes() {
        let nested: PathBuf = ["docs", "superpowers", "specs", "design.md"]
            .iter()
            .collect();

        assert_eq!(
            SpecPath::from_relative(&nested).as_str(),
            "docs/superpowers/specs/design.md"
        );
    }

    /// Verifies both identity variants expose their identifier through one accessor.
    #[test]
    fn exposes_identifier_for_both_identity_variants() {
        let declared = SpecIdentity::Declared(SpecId::new("add-auth"));
        let derived = SpecIdentity::Derived(SpecId::new("docs/specs/add-auth.md"));

        assert_eq!(declared.id(), &SpecId::new("add-auth"));
        assert_eq!(derived.id(), &SpecId::new("docs/specs/add-auth.md"));
    }

    /// Verifies a discovered document keeps every field needed by the catalog view.
    #[test]
    fn constructs_discovered_document() {
        let path = SpecPath::from_relative(&PathBuf::from("openspec/changes/add-auth/proposal.md"));
        let document = SpecDocument::new(
            SpecIdentity::Declared(SpecId::new("add-auth")),
            "OpenSpec",
            path.clone(),
            "Add authentication",
            SpecContentHash::new("2c26b46b"),
        );

        assert_eq!(
            document,
            SpecDocument {
                identity: SpecIdentity::Declared(SpecId::new("add-auth")),
                source_name: "OpenSpec".to_string(),
                path,
                title: "Add authentication".to_string(),
                content_hash: SpecContentHash::new("2c26b46b"),
            }
        );
    }

    /// Verifies a source pairs its display name with the pattern that discovers it.
    #[test]
    fn constructs_discovery_source() {
        assert_eq!(
            SpecSource::new("OpenSpec", "openspec/changes/**/*.md"),
            SpecSource {
                name: "OpenSpec".to_string(),
                glob: "openspec/changes/**/*.md".to_string(),
            }
        );
    }
}
