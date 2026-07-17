//! JSON-RPC 2.0 envelope over the Ora Wire Frame v1 (design-v3 §12.5, §12.6, §12.7, §16.1).
//!
//! Each frame payload (§12) is exactly one JSON-RPC 2.0 envelope; the frame type byte selects the
//! envelope shape (Request=1, Response=2, Notification=3). Envelopes carry raw [`serde_json::Value`]
//! params/result payloads; the typed DTOs (lifecycle §12.8, Agent §13.1) validate those payloads
//! separately, matching §12.5. Every envelope object rejects unknown fields, `jsonrpc` must equal
//! `"2.0"`, an `Option` field may be omitted but never carries an explicit `null`, and `error.data`
//! when present must be a JSON object.
//!
//! The two `id: null` sentinel responses (`-32700`/`-32600`) are best-effort diagnostics the Host
//! emits for an unassociable frame; they are constructed as raw JSON by the runtime, never through
//! [`JsonRpcErrorResponse`] (whose `id` is always a validated non-empty [`RequestId`]).

use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Map;
use thiserror::Error;
use ts_rs::TS;

use crate::agent::{AgentBusinessFailureKind, AgentEvent};
use crate::identity::JsonSafeU64;
use crate::serde_util::strict_option;

/// The Ora wire protocol version pinned by the runtime asset + bootstrap receipt (§12.8).
pub const WIRE_VERSION: u32 = 1;

/// The single supported JSON-RPC protocol version (§12.5).
pub const JSONRPC_VERSION: &str = "2.0";

// ---------------------------------------------------------------------------
// Method registry (§12.5, §12.8). `$/...` is the control/transport namespace.
// ---------------------------------------------------------------------------

/// `$/initialize` — Host→bootstrap handshake step 1 (§12.8).
pub const METHOD_INITIALIZE: &str = "$/initialize";
/// `$/activate` — Host→bootstrap handshake step 2 (§12.8).
pub const METHOD_ACTIVATE: &str = "$/activate";
/// `$/deactivate` — Host→bootstrap lifecycle stop request (§12.8).
pub const METHOD_DEACTIVATE: &str = "$/deactivate";
/// `$/exit` — Host→bootstrap lifecycle stop notification (§12.8).
pub const METHOD_EXIT: &str = "$/exit";
/// `$/cancelRequest` — Host→Plugin single-RPC transport cancel (§12.6).
pub const METHOD_CANCEL_REQUEST: &str = "$/cancelRequest";
/// `$/stream` — Plugin→Host streaming event notification (§12.7).
pub const METHOD_STREAM: &str = "$/stream";

// ---------------------------------------------------------------------------
// Error codes (§16.1). `-32000` (Agent business) is `agent::AGENT_BUSINESS_ERROR_CODE`.
// ---------------------------------------------------------------------------

/// Parse error: a complete `type=Request` frame whose UTF-8/JSON/duplicate-key/depth failed
/// (§12.5, §16.1). Best-effort `id:null` reply, then fatal.
pub const PARSE_ERROR: i32 = -32700;
/// Invalid request: JSON parseable but the envelope/batch/id/method shape is invalid (§16.1).
pub const INVALID_REQUEST: i32 = -32600;
/// Method not found: a complete legal Request for an unknown method, or a direction-violating
/// Plugin→Host Request (§16.1).
pub const METHOD_NOT_FOUND: i32 = -32601;
/// Invalid params: a known method's typed params failed validation (§16.1).
pub const INVALID_PARAMS: i32 = -32602;
/// Internal error: router/bootstrap internal exception; never a normal Agent business failure
/// (§16.1).
pub const INTERNAL_ERROR: i32 = -32603;
/// Server busy: ordinary handler admission is full (§16.1). `agent.cancelConversation` never uses
/// this lane.
pub const SERVER_BUSY: i32 = -32010;
/// Request cancelled: a cancellation accepted by the Plugin has become terminal (§16.1, §12.6).
/// LSP-derived Ora extension, not JSON-RPC core.
pub const REQUEST_CANCELLED: i32 = -32800;

/// Maximum UTF-8 bytes for an envelope `id` (§12.5: non-empty string, ≤128 bytes).
pub const REQUEST_ID_MAX_BYTES: usize = 128;

/// Errors produced while constructing or validating an envelope [`RequestId`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum JsonRpcEnvelopeError {
    #[error("request id must be non-empty")]
    EmptyId,
    #[error("request id exceeds {max} UTF-8 bytes (got {len})")]
    IdTooLong { len: usize, max: usize },
    #[error("request id must not contain NUL or C0/C1 control characters")]
    IdInvalidCharacter,
}

/// Validates an envelope `id` per §12.5: non-empty, ≤128 UTF-8 bytes, no NUL/C0/C1 control.
///
/// `/`, `\` and `:` are permitted because the envelope id is a correlation token (the canonical
/// Host form `h:<n>` contains `:`), not a path component.
fn validate_request_id(value: &str) -> Result<(), JsonRpcEnvelopeError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(JsonRpcEnvelopeError::EmptyId);
    }
    if bytes.len() > REQUEST_ID_MAX_BYTES {
        return Err(JsonRpcEnvelopeError::IdTooLong {
            len: bytes.len(),
            max: REQUEST_ID_MAX_BYTES,
        });
    }
    for ch in value.chars() {
        let code = u32::from(ch);
        if code == 0 || code <= 0x1F || (0x7F..=0x9F).contains(&code) {
            return Err(JsonRpcEnvelopeError::IdInvalidCharacter);
        }
    }
    Ok(())
}

/// A non-empty envelope `id` of at most 128 UTF-8 bytes (§12.5).
///
/// Response frames echo the request id exactly. The canonical Host outbound form is `h:<n>`
/// (see [`RequestId::host`]); the generic envelope also admits a plugin's echoed id so the Host
/// can reply to a direction-violating Plugin Request before terminating the generation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct RequestId(String);

impl RequestId {
    /// Constructs a validated envelope request id.
    pub fn try_new(value: String) -> Result<Self, JsonRpcEnvelopeError> {
        validate_request_id(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), JsonRpcEnvelopeError> {
        validate_request_id(&self.0)
    }

    /// Returns the underlying id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Builds the canonical Host outbound id `h:<n>` for a JSON-safe sequence number (§12.5).
    pub fn host(n: JsonSafeU64) -> Result<Self, JsonRpcEnvelopeError> {
        Self::try_new(format!("h:{}", n.get()))
    }
}

impl fmt::Display for RequestId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RequestId {
    type Err = JsonRpcEnvelopeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_new(value.to_string())
    }
}

/// The JSON-RPC version marker, fixed to `"2.0"` (§12.5).
///
/// Serializes as the literal `"2.0"` and rejects any other value on deserialize, so a frame whose
/// `jsonrpc` field is absent, `null`, `"1.0"` or any other shape fails envelope validation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, TS)]
#[ts(export_to = "plugin-protocol.ts", type = "\"2.0\"")]
pub struct JsonRpcVersion;

impl JsonRpcVersion {
    /// The literal version string this marker always represents.
    pub const VALUE: &'static str = JSONRPC_VERSION;
}

impl Serialize for JsonRpcVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(JSONRPC_VERSION)
    }
}

impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        match value {
            serde_json::Value::String(text) if text == JSONRPC_VERSION => Ok(JsonRpcVersion),
            other => Err(serde::de::Error::custom(format!(
                "jsonrpc must equal \"2.0\", got {other}"
            ))),
        }
    }
}

// `error.data` is an optional JSON object (§12.5): an omitted field is allowed, but an explicit
// `null` or a non-object value is a contract violation. This is enforced by `strict_option`
// (rejects `null`) combined with `Map<String, Value>` deserialization (rejects non-objects).

/// A JSON-RPC error object carried in a Response frame (§12.5, §16.1).
///
/// `code` is an i32 in the JSON-RPC 2.0 / Ora extension range; `data`, when present, is always a
/// JSON object (e.g. the `-32000` business error payload `{ kind, retryable, details? }`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct JsonRpcError {
    #[ts(type = "number")]
    pub code: i32,
    pub message: String,
    #[serde(
        default,
        deserialize_with = "strict_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "Record<string, unknown>")]
    pub data: Option<Map<String, serde_json::Value>>,
}

impl JsonRpcError {
    /// Builds a business-free error with no `data`.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

/// A JSON-RPC Request envelope (§12.5): `jsonrpc`, `id`, `method`, optional `params`.
///
/// `params` carries the raw payload; the router deserializes it into the method's typed DTO
/// (lifecycle §12.8 or Agent §13.1) and applies its strict validation separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct JsonRpcRequest {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub method: String,
    #[serde(
        default,
        deserialize_with = "strict_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "unknown")]
    pub params: Option<serde_json::Value>,
}

/// A JSON-RPC success Response envelope (§12.5): `jsonrpc`, `id`, `result`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct JsonRpcSuccessResponse {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    #[ts(type = "unknown")]
    pub result: serde_json::Value,
}

/// A JSON-RPC error Response envelope (§12.5): `jsonrpc`, `id`, `error`.
///
/// `id` echoes the in-flight request id exactly (always a non-empty string here). The `id: null`
/// sentinel diagnostics for unparseable requests are emitted as raw JSON by the runtime, not
/// through this typed envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct JsonRpcErrorResponse {
    pub jsonrpc: JsonRpcVersion,
    pub id: RequestId,
    pub error: JsonRpcError,
}

/// A JSON-RPC Notification envelope (§12.5): `jsonrpc`, `method`, optional `params`, no `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct JsonRpcNotification {
    pub jsonrpc: JsonRpcVersion,
    pub method: String,
    #[serde(
        default,
        deserialize_with = "strict_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional, type = "unknown")]
    pub params: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Raw JSON value/object types and `-32000` business-error data (§13.2, §16.1)
// ---------------------------------------------------------------------------

/// A recursive JSON value type for the SDK, exported as the TS `JsonValue` union (§13.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export_to = "plugin-protocol.ts",
    type = "null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }"
)]
pub struct JsonValue;

/// A raw JSON object (`Record<string, JsonValue>`), used for business-error `details`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "Record<string, JsonValue>")]
pub struct JsonObject(BTreeMap<String, serde_json::Value>);

impl JsonObject {
    /// Constructs a raw JSON object from a sorted map.
    pub fn new(map: BTreeMap<String, serde_json::Value>) -> Self {
        Self(map)
    }

    /// Returns the underlying map.
    pub fn as_map(&self) -> &BTreeMap<String, serde_json::Value> {
        &self.0
    }
}

/// The `data` object for an `-32000` agent business failure (§16.1, §13.2).
///
/// Deserialized from [`JsonRpcError::data`] when `code` is `agent::AGENT_BUSINESS_ERROR_CODE`.
/// Authors cannot create `ProviderFailure`; the bootstrap synthesizes it for raw throws/rejects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentBusinessErrorData {
    pub kind: AgentBusinessFailureKind,
    pub retryable: bool,
    #[serde(
        default,
        deserialize_with = "strict_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub details: Option<JsonObject>,
}

/// `$/stream` notification params: one event for a Host-initiated streaming request (§12.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct StreamNotificationParams {
    pub id: RequestId,
    pub seq: JsonSafeU64,
    pub value: AgentEvent,
}

/// `$/cancelRequest` notification params: cancels one in-flight request id (§12.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct CancelRequestParams {
    pub id: RequestId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn request_id_accepts_host_form_and_rejects_empty_long_and_control() {
        assert!(validate_request_id("h:1").is_ok());
        assert!(validate_request_id("h:9007199254740991").is_ok());
        assert_eq!(validate_request_id(""), Err(JsonRpcEnvelopeError::EmptyId));
        assert!(matches!(
            validate_request_id(&"a".repeat(129)),
            Err(JsonRpcEnvelopeError::IdTooLong { .. })
        ));
        assert_eq!(
            validate_request_id("a\tb"),
            Err(JsonRpcEnvelopeError::IdInvalidCharacter)
        );
        let n = JsonSafeU64::try_new(42).unwrap_or_else(|error| panic!("valid number: {error}"));
        let host = RequestId::host(n).unwrap_or_else(|error| panic!("host id: {error}"));
        assert_eq!(host.as_str(), "h:42");
    }

    #[test]
    fn jsonrpc_version_round_trips_as_literal_and_rejects_other_values() {
        assert_eq!(
            serde_json::to_value(JsonRpcVersion)
                .unwrap_or_else(|error| panic!("serialize: {error}")),
            json!("2.0")
        );
        let _: JsonRpcVersion = serde_json::from_value(json!("2.0"))
            .unwrap_or_else(|error| panic!("deserialize 2.0: {error}"));
        assert!(serde_json::from_value::<JsonRpcVersion>(json!("1.0")).is_err());
        assert!(serde_json::from_value::<JsonRpcVersion>(json!(null)).is_err());
        assert!(serde_json::from_value::<JsonRpcVersion>(json!(2)).is_err());
    }

    #[test]
    fn request_envelope_round_trips_and_rejects_unknown_and_null_params() {
        let request = JsonRpcRequest {
            jsonrpc: JsonRpcVersion,
            id: RequestId::try_new("h:1".to_string())
                .unwrap_or_else(|error| panic!("valid id: {error}")),
            method: METHOD_INITIALIZE.to_string(),
            params: Some(json!({ "wireVersion": 1 })),
        };
        let value =
            serde_json::to_value(&request).unwrap_or_else(|error| panic!("serialize: {error}"));
        assert_eq!(
            value,
            json!({
                "jsonrpc": "2.0",
                "id": "h:1",
                "method": "$/initialize",
                "params": { "wireVersion": 1 }
            })
        );
        let parsed: JsonRpcRequest =
            serde_json::from_value(value).unwrap_or_else(|error| panic!("deserialize: {error}"));
        assert_eq!(parsed.method, METHOD_INITIALIZE);

        // Unknown top-level field is rejected.
        let mut with_extra =
            serde_json::to_value(&request).unwrap_or_else(|error| panic!("serialize: {error}"));
        if let serde_json::Value::Object(ref mut map) = with_extra {
            map.insert("rogue".to_string(), json!("nope"));
        }
        assert!(serde_json::from_value::<JsonRpcRequest>(with_extra).is_err());

        // Explicit null params is rejected; omission is allowed.
        assert!(
            serde_json::from_value::<JsonRpcRequest>(json!({
                "jsonrpc": "2.0", "id": "h:1", "method": "$/exit", "params": null
            }))
            .is_err()
        );
        let no_params: JsonRpcRequest = serde_json::from_value(json!({
            "jsonrpc": "2.0", "id": "h:1", "method": "$/exit"
        }))
        .unwrap_or_else(|error| panic!("deserialize no-params: {error}"));
        assert!(no_params.params.is_none());
    }

    #[test]
    fn error_envelope_enforces_data_object() {
        let error: JsonRpcError = serde_json::from_value(json!({
            "code": -32000, "message": "boom", "data": { "kind": "agentUnavailable", "retryable": false }
        }))
        .unwrap_or_else(|error| panic!("deserialize error data: {error}"));
        assert!(error.data.is_some());
        assert!(
            serde_json::from_value::<JsonRpcError>(json!({
                "code": -32000, "message": "boom", "data": 7
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<JsonRpcError>(json!({
                "code": -32000, "message": "boom", "data": null
            }))
            .is_err()
        );
    }

    #[test]
    fn notification_envelope_has_no_id() {
        let notification = JsonRpcNotification {
            jsonrpc: JsonRpcVersion,
            method: METHOD_EXIT.to_string(),
            params: None,
        };
        assert_eq!(
            serde_json::to_value(&notification)
                .unwrap_or_else(|error| panic!("serialize: {error}")),
            json!({ "jsonrpc": "2.0", "method": "$/exit" })
        );
        assert!(
            serde_json::from_value::<JsonRpcNotification>(json!({
                "jsonrpc": "2.0", "method": "$/exit", "id": "h:1"
            }))
            .is_err()
        );
    }
}
