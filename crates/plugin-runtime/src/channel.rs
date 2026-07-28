use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ora_contracts::{acp::notification::SessionNotification, plugin_methods};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, broadcast, oneshot};

use crate::error::PluginRuntimeError;

/// Frame type for JSON-RPC payload (currently the only supported type).
const FRAME_TYPE_JSON: u8 = 1;

/// Maximum payload size (16 MB) to guard against malicious or corrupt frames.
const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;

/// Carries one JSON-RPC response routed back to the waiting caller.
enum ChannelResult {
    Ok(Value),
    Err(String),
}

/// Plugin-channel binary-frame JSON-RPC client over a plugin process's stdio.
///
/// Each message is carried in a binary frame: `[type: u8][length: i32 big-endian][payload: n bytes]`.
/// The 5-byte header carries a `type` byte (payload content selector, currently always 1 = JSON)
/// and a `length` (payload byte count, not counting the header). Total frame size = 5 + length.
///
/// The host writes type=1 (JSON-RPC) frames to the plugin's stdin and routes the plugin's
/// type=1 responses (by request id) and `agent/sessionUpdate` notifications (to a broadcast)
/// read from its stdout. Non-type-1 frames are read and discarded (future types dispatch here).
pub struct PluginChannel {
    stdin: Arc<Mutex<Box<dyn tokio::io::AsyncWrite + Unpin + Send + 'static>>>,
    next_id: AtomicU64,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<ChannelResult>>>>,
    notifications: broadcast::Sender<SessionNotification>,
}

impl PluginChannel {
    /// Builds the channel from the plugin process's stdio pipes and spawns the stdout reader.
    pub fn new(
        stdin: Box<dyn tokio::io::AsyncWrite + Unpin + Send + 'static>,
        stdout: Box<dyn tokio::io::AsyncRead + Unpin + Send + 'static>,
        notifications: broadcast::Sender<SessionNotification>,
    ) -> Arc<Self> {
        let stdin = Arc::new(Mutex::new(stdin));
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let channel = Arc::new(Self {
            stdin,
            next_id: AtomicU64::new(1),
            pending,
            notifications,
        });
        channel.clone().spawn_reader(stdout);
        channel
    }

    /// Returns a receiver for `agent/sessionUpdate` notifications emitted by this plugin.
    pub fn subscribe(&self) -> broadcast::Receiver<SessionNotification> {
        self.notifications.subscribe()
    }

    /// Sends a JSON-RPC request (as a type=1 binary frame) and awaits the matching response.
    pub async fn request<Req, Res>(
        &self,
        method: &str,
        params: Req,
    ) -> Result<Res, PluginRuntimeError>
    where
        Req: Serialize,
        Res: DeserializeOwned,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed).to_string();
        let params_value = serde_json::to_value(&params).map_err(PluginRuntimeError::from_serde)?;
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params_value,
        });
        let payload = serde_json::to_vec(&message).map_err(PluginRuntimeError::from_serde)?;

        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), sender);

        {
            let mut stdin = self.stdin.lock().await;
            let frame = build_frame(FRAME_TYPE_JSON, &payload)?;
            stdin
                .write_all(&frame)
                .await
                .map_err(PluginRuntimeError::from_io)?;
        }

        let result = receiver.await.map_err(|_| PluginRuntimeError::Channel {
            message: "plugin dropped its response".to_string(),
        })?;

        match result {
            ChannelResult::Ok(value) => {
                serde_json::from_value::<Res>(value).map_err(PluginRuntimeError::from_serde)
            }
            ChannelResult::Err(message) => Err(PluginRuntimeError::PluginError { message }),
        }
    }

    /// Spawns the background task that reads binary frames and routes responses / notifications.
    fn spawn_reader(
        self: Arc<Self>,
        stdout: Box<dyn tokio::io::AsyncRead + Unpin + Send + 'static>,
    ) {
        let pending = self.pending.clone();
        let notifications = self.notifications.clone();
        tokio::spawn(async move {
            let mut reader = stdout;
            loop {
                // Read 5-byte frame header: [type: u8][length: i32 big-endian].
                let mut header = [0u8; 5];
                if reader.read_exact(&mut header).await.is_err() {
                    break; // EOF or read error — stream closed.
                }
                let frame_type = header[0];
                let length = i32::from_be_bytes([header[1], header[2], header[3], header[4]]);
                if length < 0 || (length as usize) > MAX_PAYLOAD_SIZE {
                    continue; // Skip invalid frame (negative or oversized length).
                }

                // Read the payload (exactly `length` bytes; read_exact handles 分包/粘包).
                let mut payload = vec![0u8; length as usize];
                if reader.read_exact(&mut payload).await.is_err() {
                    break; // EOF mid-payload — stream truncated.
                }

                // Dispatch by frame type. Currently only type=1 (JSON) is handled.
                if frame_type != FRAME_TYPE_JSON {
                    continue; // Non-JSON frame: read + discard (future types dispatch here).
                }

                let Ok(message) = serde_json::from_slice::<Value>(&payload) else {
                    continue; // Skip malformed JSON payload.
                };

                if let Some(id) = message.get("id").and_then(Value::as_str) {
                    let routed = if let Some(error) = message.get("error") {
                        ChannelResult::Err(error.to_string())
                    } else {
                        ChannelResult::Ok(message.get("result").cloned().unwrap_or(Value::Null))
                    };
                    if let Some(sender) = pending.lock().await.remove(id) {
                        let _ = sender.send(routed);
                    }
                } else if let Some(method) = message.get("method").and_then(Value::as_str)
                    && method == plugin_methods::AGENT_SESSION_UPDATE
                    && let Some(params) = message.get("params")
                    && let Ok(notification) =
                        serde_json::from_value::<SessionNotification>(params.clone())
                {
                    let _ = notifications.send(notification);
                }
            }
        });
    }
}

/// Builds a binary frame: `[type: u8][length: i32 big-endian][payload: n bytes]`.
///
/// Uses manual byte writing (no Rust struct) to avoid alignment padding — a struct with
/// `{ type: u8, length: i32 }` would be 8 bytes (3 padding after u8), but the wire format
/// requires 5 bytes. Manual `push` + `to_be_bytes` produces exactly 5 header bytes.
fn build_frame(frame_type: u8, payload: &[u8]) -> Result<Vec<u8>, PluginRuntimeError> {
    let length = i32::try_from(payload.len()).map_err(|_| PluginRuntimeError::Channel {
        message: format!("payload too large for i32 length: {} bytes", payload.len()),
    })?;
    let mut frame = Vec::with_capacity(5 + payload.len());
    frame.push(frame_type); // type: 1 byte
    frame.extend_from_slice(&length.to_be_bytes()); // length: 4 bytes big-endian
    frame.extend_from_slice(payload); // payload
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ora_contracts::{InitializeRequest, InitializeResponse, PluginKind, plugin_methods};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    /// Verifies the channel correlates a request id to the plugin's matching binary-frame response.
    #[tokio::test]
    async fn routes_initialize_request_and_response_by_id() {
        let (channel_stdin, plugin_stdin_reader) = duplex(1024);
        let (plugin_stdout_writer, channel_stdout) = duplex(1024);
        let (notifications, _) = broadcast::channel(16);
        let channel = PluginChannel::new(
            Box::new(channel_stdin),
            Box::new(channel_stdout),
            notifications,
        );

        // Fake plugin: read one binary frame, echo a response carrying the same id.
        tokio::spawn(async move {
            let mut reader = plugin_stdin_reader;
            let mut writer = plugin_stdout_writer;

            // Read frame header (5 bytes).
            let mut header = [0u8; 5];
            reader.read_exact(&mut header).await.unwrap();
            let length = i32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;

            // Read payload.
            let mut payload = vec![0u8; length];
            reader.read_exact(&mut payload).await.unwrap();
            let message: Value = serde_json::from_slice(&payload).unwrap();
            let id = message.get("id").and_then(Value::as_str).unwrap_or("0");

            // Build + write response frame.
            let response = json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "kind": "agent", "version": "0.1.0" },
            });
            let response_bytes = serde_json::to_vec(&response).unwrap();
            let frame = build_frame(FRAME_TYPE_JSON, &response_bytes).unwrap();
            writer.write_all(&frame).await.unwrap();
        });

        let response: InitializeResponse = channel
            .request(
                plugin_methods::INITIALIZE,
                InitializeRequest {
                    protocol_version: "0.1.0".to_string(),
                },
            )
            .await
            .unwrap();

        assert_eq!(response.kind, PluginKind::Agent);
        assert_eq!(response.version, "0.1.0");
    }
}
