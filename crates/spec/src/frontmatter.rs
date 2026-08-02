const FENCE: &str = "---";

/// Holds the subset of YAML frontmatter that spec discovery understands.
///
/// Only scalar keys are recognized. A full YAML parser is deliberately avoided: spec
/// documents are authored by many unrelated tools, and any key beyond these two is
/// metadata Ora has no business interpreting.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Frontmatter {
    pub(crate) id: Option<String>,
    pub(crate) title: Option<String>,
}

/// Extracts the recognized frontmatter keys and the document body that follows them.
///
/// Returns the whole input as the body when no leading fence is present, which is the
/// common case for hand-written specs and for documents produced by tools that do not
/// emit frontmatter at all.
pub(crate) fn split_frontmatter(content: &str) -> (Frontmatter, &str) {
    let Some(rest) = strip_opening_fence(content) else {
        return (Frontmatter::default(), content);
    };
    let Some((block, body)) = split_at_closing_fence(rest) else {
        return (Frontmatter::default(), content);
    };

    (parse_recognized_keys(block), body)
}

/// Derives the document title, preferring declared frontmatter over the first heading.
///
/// The final fallback is the file stem so that every discovered document has a label,
/// even one that is empty or contains only prose.
pub(crate) fn resolve_title(frontmatter: &Frontmatter, body: &str, file_stem: &str) -> String {
    if let Some(title) = frontmatter.title.as_ref().filter(|title| !title.is_empty()) {
        return title.clone();
    }

    body.lines()
        .find_map(|line| {
            let heading = line.trim_start().strip_prefix("# ")?;
            let heading = heading.trim();
            (!heading.is_empty()).then(|| heading.to_string())
        })
        .unwrap_or_else(|| file_stem.to_string())
}

/// Consumes the opening fence together with its line ending.
fn strip_opening_fence(content: &str) -> Option<&str> {
    let rest = content.strip_prefix(FENCE)?;
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
}

/// Splits the frontmatter block from the body at the first closing fence line.
fn split_at_closing_fence(rest: &str) -> Option<(&str, &str)> {
    let mut offset = 0;

    for line in rest.split_inclusive('\n') {
        if line.trim_end() == FENCE {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }

    // A trailing fence without a line ending still closes the block.
    (rest.trim_end() == FENCE).then_some((&rest[..0], ""))
}

/// Reads the recognized scalar keys out of a frontmatter block.
fn parse_recognized_keys(block: &str) -> Frontmatter {
    let mut frontmatter = Frontmatter::default();

    for line in block.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        // Indented keys belong to a nested mapping whose meaning Ora does not define.
        if key.starts_with(char::is_whitespace) {
            continue;
        }

        let value = unquote(value.trim());
        match key.trim() {
            "id" => frontmatter.id = Some(value),
            "title" => frontmatter.title = Some(value),
            _ => {}
        }
    }

    frontmatter
}

/// Removes one layer of matching YAML quotes from a scalar value.
fn unquote(value: &str) -> String {
    for quote in ['"', '\''] {
        if let Some(inner) = value
            .strip_prefix(quote)
            .and_then(|v| v.strip_suffix(quote))
        {
            return inner.to_string();
        }
    }

    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::{Frontmatter, resolve_title, split_frontmatter};
    use pretty_assertions::assert_eq;

    /// Verifies declared keys are read while unrelated and nested keys are ignored.
    #[test]
    fn reads_recognized_keys_only() {
        let content = "---\nid: add-auth\ntitle: \"Add authentication\"\nstatus: draft\nmeta:\n  id: nested\n---\n# Heading\n\nBody\n";

        assert_eq!(
            split_frontmatter(content),
            (
                Frontmatter {
                    id: Some("add-auth".to_string()),
                    title: Some("Add authentication".to_string()),
                },
                "# Heading\n\nBody\n"
            )
        );
    }

    /// Verifies a document without frontmatter is returned untouched.
    #[test]
    fn preserves_documents_without_frontmatter() {
        let content = "# Heading\n\nBody\n";

        assert_eq!(
            split_frontmatter(content),
            (Frontmatter::default(), content)
        );
    }

    /// Verifies an unterminated fence is treated as body rather than as metadata.
    #[test]
    fn treats_unterminated_fence_as_body() {
        let content = "---\nid: add-auth\n# Heading\n";

        assert_eq!(
            split_frontmatter(content),
            (Frontmatter::default(), content)
        );
    }

    /// Verifies title resolution walks from declared metadata down to the file stem.
    #[test]
    fn resolves_title_through_every_fallback() {
        let declared = Frontmatter {
            id: None,
            title: Some("Declared".to_string()),
        };

        assert_eq!(
            resolve_title(&declared, "# Heading\n", "design"),
            "Declared".to_string()
        );
        assert_eq!(
            resolve_title(&Frontmatter::default(), "intro\n\n#  Heading  \n", "design"),
            "Heading".to_string()
        );
        assert_eq!(
            resolve_title(&Frontmatter::default(), "no headings here\n", "design"),
            "design".to_string()
        );
    }
}
