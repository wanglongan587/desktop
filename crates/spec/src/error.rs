use std::path::PathBuf;
use thiserror::Error;

/// Reports failures that prevent Ora from discovering or reading spec documents.
///
/// Failures stay coarse on purpose: the catalog is a read-only projection of the
/// workspace, so callers only ever need to distinguish "the workspace is unusable" from
/// "this document is not part of the catalog".
#[derive(Debug, Error)]
pub enum SpecError {
    #[error("spec workspace {path:?} is unavailable")]
    WorkspaceUnavailable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("spec source configuration {path:?} is invalid")]
    InvalidSourceConfiguration {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error("spec source pattern {pattern:?} is invalid")]
    InvalidSourcePattern {
        pattern: String,
        #[source]
        source: globset::Error,
    },
    #[error("spec document {path:?} is not part of the catalog")]
    DocumentNotFound { path: String },
    #[error("failed to start watching spec directories under {path:?}")]
    WatchFailed {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },
}
