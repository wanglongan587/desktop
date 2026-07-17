//! Content tree digest (design-v3 §6.3).
//!
//! The digest proves "the managed copy has not changed since install" (not publisher identity, not
//! a sandbox — §6.3). The canonical algorithm is domain-separated, regular-files-only (excluding
//! Host-generated `.ora/`), path-sorted, and length-prefixed so it cannot collide with other
//! digest protocols or reorderings:
//!
//! 1. seed `SHA-256` with `UTF8("ora-plugin-tree-v1") || 0x00`;
//! 2. sort entries by the UTF-8 bytes of their normalized `/`-relative path ascending;
//! 3. for each entry, feed `path_len:u32 BE | path_bytes | file_len:u64 BE | file_sha256:32 bytes`;
//! 4. the tree digest is the final `SHA-256` of that stream.
//!
//! Filesystem enumeration, ADS/reparse rejection and `.ora/` exclusion happen in the scanner; this
//! module is the pure algorithm over a prepared `&[FileEntry]`.

use std::fmt;

use sha2::{Digest, Sha256};

/// Domain separator: `UTF8("ora-plugin-tree-v1")` followed by a `0x00` byte (§6.3 step 1).
const DOMAIN_SEPARATOR: &[u8] = b"ora-plugin-tree-v1\x00";

/// One regular file's contribution to the tree digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    /// Normalized relative path (`/`-separated, UTF-8, excluding `.ora/`).
    pub relative_path: String,
    /// File length in bytes.
    pub file_len: u64,
    /// The file's own `SHA-256` (32 bytes).
    pub file_sha256: [u8; 32],
}

/// A content tree digest (32 bytes), rendered as `sha256:<64 lowercase hex>` (§6.2/§6.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TreeDigest([u8; 32]);

impl TreeDigest {
    /// Returns the raw 32 digest bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Returns the display form `sha256:<64 lowercase hex>`.
    pub fn as_display(&self) -> String {
        format!("sha256:{}", bytes_to_hex(&self.0))
    }
}

impl fmt::Display for TreeDigest {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.as_display())
    }
}

/// Computes the tree digest over the given file entries (§6.3).
///
/// Entries are sorted internally by relative-path UTF-8 bytes, so the digest is independent of the
/// caller's enumeration order.
pub fn compute_tree_digest(entries: &[FileEntry]) -> TreeDigest {
    let mut sorted: Vec<&FileEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.relative_path.as_bytes().cmp(b.relative_path.as_bytes()));

    let mut hasher = Sha256::new();
    hasher.update(DOMAIN_SEPARATOR);
    for entry in sorted {
        let path_bytes = entry.relative_path.as_bytes();
        let path_len = u32::try_from(path_bytes.len())
            .unwrap_or_else(|_| panic!("relative path length exceeds u32: {}", path_bytes.len()));
        hasher.update(path_len.to_be_bytes());
        hasher.update(path_bytes);
        hasher.update(entry.file_len.to_be_bytes());
        hasher.update(entry.file_sha256);
    }
    let result = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    TreeDigest(bytes)
}

/// Renders 32 bytes as 64 lowercase hex characters (no `sha256:` prefix).
fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn entry(path: &str, len: u64, sha: [u8; 32]) -> FileEntry {
        FileEntry {
            relative_path: path.to_string(),
            file_len: len,
            file_sha256: sha,
        }
    }

    #[test]
    fn empty_entries_yield_a_stable_digest() {
        let d1 = compute_tree_digest(&[]);
        let d2 = compute_tree_digest(&[]);
        assert_eq!(d1, d2);
        assert_eq!(d1.as_display(), d2.as_display());
        // Display form is the canonical sha256:<hex>.
        let display = d1.as_display();
        assert!(display.starts_with("sha256:"));
        assert_eq!(display.len(), "sha256:".len() + 64);
    }

    #[test]
    fn digest_is_independent_of_enumeration_order() {
        let zero = [0u8; 32];
        let one = {
            let mut b = [0u8; 32];
            b[31] = 1;
            b
        };
        let a = entry("dist/index.js", 10, zero);
        let b = entry("package.json", 3, one);
        let mut entries = [a, b];
        let forward = compute_tree_digest(&entries);
        entries.reverse();
        let reverse = compute_tree_digest(&entries);
        assert_eq!(forward, reverse);
    }

    #[test]
    fn different_content_or_length_yields_different_digests() {
        let zero = [0u8; 32];
        let one = {
            let mut b = [0u8; 32];
            b[31] = 1;
            b
        };
        let e1 = entry("package.json", 3, zero);
        // Different length, same path + sha.
        let e2 = entry("package.json", 4, zero);
        assert_ne!(
            compute_tree_digest(std::slice::from_ref(&e1)),
            compute_tree_digest(std::slice::from_ref(&e2))
        );
        // Different sha, same path + length.
        let e3 = entry("package.json", 3, one);
        assert_ne!(
            compute_tree_digest(std::slice::from_ref(&e1)),
            compute_tree_digest(std::slice::from_ref(&e3))
        );
        // Different path.
        let e4 = entry("other.json", 3, zero);
        assert_ne!(
            compute_tree_digest(std::slice::from_ref(&e1)),
            compute_tree_digest(std::slice::from_ref(&e4))
        );
    }

    #[test]
    fn domain_separator_distinguishes_from_raw_concatenation() {
        // The tree digest must NOT equal a plain SHA-256 of the item bytes without the domain
        // separator, proving the separator is part of the canonical input (§6.3 step 1).
        let zero = [0u8; 32];
        let file = entry("package.json", 3, zero);

        let mut raw = Sha256::new();
        let path_bytes = file.relative_path.as_bytes();
        raw.update((path_bytes.len() as u32).to_be_bytes());
        raw.update(path_bytes);
        raw.update(file.file_len.to_be_bytes());
        raw.update(file.file_sha256);
        let mut raw_bytes = [0u8; 32];
        raw_bytes.copy_from_slice(&raw.finalize());

        assert_ne!(
            compute_tree_digest(std::slice::from_ref(&file)).as_bytes(),
            &raw_bytes
        );
    }
}
