//! Minimal ACP v1 stdio peer used by Ora's provider-neutral agent runtime.

mod peer;

pub use peer::{AcpClient, AcpControl, AcpError, AcpPeer, PermissionRequest};
