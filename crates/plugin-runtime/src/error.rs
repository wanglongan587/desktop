use thiserror::Error;

/// Captures plugin-runtime failures surfaced to adapters (Tauri commands / Web handlers).
#[derive(Debug, Error)]
pub enum PluginRuntimeError {
    #[error("plugin is not active: {plugin_id}")]
    NotActive { plugin_id: String },
    #[error("plugin is already active: {plugin_id}")]
    AlreadyActive { plugin_id: String },
    #[error("failed to spawn plugin process: {message}")]
    Spawn { message: String },
    #[error("plugin channel transport error: {message}")]
    Channel { message: String },
    #[error("plugin returned an error response: {message}")]
    PluginError { message: String },
}

impl PluginRuntimeError {
    /// Wraps a serde failure with stable transport context.
    pub(crate) fn from_serde(error: serde_json::Error) -> Self {
        Self::Channel {
            message: error.to_string(),
        }
    }

    /// Wraps an IO failure during channel writes.
    pub(crate) fn from_io(error: std::io::Error) -> Self {
        Self::Channel {
            message: error.to_string(),
        }
    }
}
