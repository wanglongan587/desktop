use ora_contracts::{SpecDocument, SpecSource};
use ora_domain::{SpecDocument as DomainSpecDocument, SpecSource as DomainSpecSource};

/// Projects one discovered document into the shared contract view.
///
/// How the identity was obtained is intentionally dropped: the catalog only needs a
/// stable key, and the declared/derived distinction matters to provenance rather than to
/// presentation.
pub(crate) fn map_spec_document(document: DomainSpecDocument) -> SpecDocument {
    SpecDocument {
        id: document.identity.id().to_string(),
        source_name: document.source_name,
        path: document.path.to_string(),
        title: document.title,
    }
}

/// Projects one discovery source into the shared contract view.
pub(crate) fn map_spec_source(source: DomainSpecSource) -> SpecSource {
    SpecSource {
        name: source.name,
        glob: source.glob,
    }
}
