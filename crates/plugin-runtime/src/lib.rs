//! Live plugin runtime: spawns plugin processes, drives the plugin-channel handshake, and
//! forwards agent operations to the right plugin by id.
//!
//! This crate owns only the live runtime (spawn, channel, lifecycle, notification routing).
//! Persisted plugin metadata and the local scan/install flow live in `ora-application`;
//! the transport adapter (Tauri commands + event emit, or Web SSE) lives in the app layer.

mod channel;
mod error;
mod manager;

pub use channel::PluginChannel;
pub use error::PluginRuntimeError;
pub use manager::PluginRuntimeManager;
