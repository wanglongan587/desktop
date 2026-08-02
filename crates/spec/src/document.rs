use crate::frontmatter::{resolve_title, split_frontmatter};
use ora_domain::{SpecContentHash, SpecDocument, SpecId, SpecIdentity, SpecPath};
use sha2::{Digest, Sha256};

/// Builds the catalog entry for one discovered file.
///
/// Identity is declared when the document names itself and derived from its path
/// otherwise. Deriving from the path rather than from a random value is what keeps a
/// document stable across rescans, and normalizing the path is what keeps it stable
/// across platforms.
pub(crate) fn build_document(
    source_name: &str,
    relative_path: SpecPath,
    file_stem: &str,
    content: &str,
) -> SpecDocument {
    let (frontmatter, body) = split_frontmatter(content);
    let identity = match frontmatter.id.as_ref().filter(|id| !id.is_empty()) {
        Some(declared) => SpecIdentity::Declared(SpecId::new(declared)),
        None => SpecIdentity::Derived(SpecId::new(relative_path.as_str())),
    };

    SpecDocument::new(
        identity,
        source_name,
        relative_path,
        resolve_title(&frontmatter, body, file_stem),
        hash_content(content),
    )
}

/// Fingerprints raw document bytes so freshness never depends on filesystem timestamps.
fn hash_content(content: &str) -> SpecContentHash {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    let encoded = digest.iter().fold(
        String::with_capacity(digest.len() * 2),
        |mut encoded, byte| {
            encoded.push_str(&format!("{byte:02x}"));
            encoded
        },
    );

    SpecContentHash::new(encoded)
}

#[cfg(test)]
mod tests {
    use super::{build_document, hash_content};
    use ora_domain::{SpecDocument, SpecId, SpecIdentity, SpecPath};
    use pretty_assertions::assert_eq;
    use std::path::Path;

    /// Verifies a self-naming document keeps the identifier it declares.
    #[test]
    fn keeps_declared_identity() {
        let path = SpecPath::from_relative(Path::new("docs/specs/design.md"));

        assert_eq!(
            build_document(
                "Docs",
                path.clone(),
                "design",
                "---\nid: add-auth\n---\n# Add authentication\n",
            ),
            SpecDocument::new(
                SpecIdentity::Declared(SpecId::new("add-auth")),
                "Docs",
                path,
                "Add authentication",
                hash_content("---\nid: add-auth\n---\n# Add authentication\n"),
            )
        );
    }

    /// Verifies a document without a declared identifier falls back to its normalized path.
    #[test]
    fn derives_identity_from_path() {
        let path = SpecPath::from_relative(Path::new("docs/specs/design.md"));

        assert_eq!(
            build_document("Docs", path.clone(), "design", "# Design\n"),
            SpecDocument::new(
                SpecIdentity::Derived(SpecId::new("docs/specs/design.md")),
                "Docs",
                path,
                "Design",
                hash_content("# Design\n"),
            )
        );
    }

    /// Verifies identical bytes hash identically while any change alters the fingerprint.
    #[test]
    fn fingerprints_content_deterministically() {
        assert_eq!(hash_content("# Design\n"), hash_content("# Design\n"));
        assert_ne!(hash_content("# Design\n"), hash_content("# Design \n"));
    }
}
