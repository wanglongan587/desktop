//! Plugin identity and validated leaf newtypes (design-v3 §5.2, §13.1 leaf table).
//!
//! These newtypes are the boundary between untrusted JSON/path strings and the typed plugin
//! subsystem. They encode as transparent JSON primitives (never `{"value": ...}` objects) and
//! carry construction-time validation so downstream code can rely on invariants. The wire codec
//! deserializes transparently and then calls [`validate`] (or constructs via `try_new`) before a
//! value may influence authorization or dispatch, matching §13.1 ("bootstrap 在编码前严格验证每个
//! provider event/result").

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;
use ts_rs::TS;

/// Maximum JSON-safe unsigned 64-bit integer carried on the Ora wire (`2^53 - 1`).
///
/// Crossing JSON's safe-integer boundary would silently lose precision in JavaScript, so values
/// above this constant are rejected at the wire boundary rather than clamped.
pub const JSON_SAFE_U64_MAX: u64 = (1u64 << 53) - 1;

/// Maximum bytes for a [`PluginId`].
pub const PLUGIN_ID_MAX_BYTES: usize = 128;

/// Maximum bytes for an opaque plugin identity (§13.1 leaf table: 1..=256).
pub const OPAQUE_ID_MAX_BYTES: usize = 256;

/// Maximum bytes for an [`AgentConfigurationKey`] (§13.1: 1..=512 ASCII).
pub const AGENT_CONFIG_KEY_MAX_BYTES: usize = 512;

/// Maximum bytes for a [`HostResolvedAbsolutePath`] (§13.1: 1..=32 KiB).
pub const HOST_RESOLVED_PATH_MAX_BYTES: usize = 32 * 1024;

/// Maximum bytes for an [`AgentPrompt`] (§13.1: 1..=1 MiB).
pub const AGENT_PROMPT_MAX_BYTES: usize = 1024 * 1024;

/// Maximum bytes for an [`Rfc3339Timestamp`] (§13.1: max 64 ASCII).
pub const RFC3339_MAX_BYTES: usize = 64;

/// Errors produced when constructing or validating plugin identity leaf values.
///
/// Variants are stable enough to surface as structured diagnostics; attacker-controlled detail is
/// not embedded beyond what the leaf table permits.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum IdentityError {
    #[error("value must be non-empty")]
    Empty,
    #[error("value exceeds {max} UTF-8 bytes (got {len})")]
    TooLong { len: usize, max: usize },
    #[error("value must not contain NUL, C0/C1 control characters, '/', '\\', or ':'")]
    InvalidCharacter,
    #[error("value must be unchanged after trimming leading/trailing whitespace")]
    HasSurroundingWhitespace,
    #[error("value does not match required grammar: {reason}")]
    Grammar { reason: &'static str },
    #[error("value must be a JSON integer in 0..=2^53-1")]
    NotJsonSafeInteger,
    #[error("value must be a finite JSON number (NaN/Infinity rejected)")]
    NotFiniteNumber,
    #[error("value must be an integer in {lo}..={hi}")]
    NotInRange { lo: u64, hi: u64 },
    #[error("value must be a canonical lowercase UUID (8-4-4-4-12 hex)")]
    InvalidUuid,
    #[error("value must be a valid RFC 3339 timestamp with 'Z' or numeric offset")]
    InvalidRfc3339,
    #[error("value contains a reserved Windows device name")]
    ReservedDeviceName,
    #[error("value must not contain a trailing dot, trailing space, colon or ADS syntax")]
    ReservedPathSyntax,
}

/// Validates the shared opaque identity rules (§13.1 leaf table): 1..=256 UTF-8 bytes,
/// trim-invariant, no NUL/C0/C1 control, no `/`, `\`, `:`.
fn validate_opaque_id(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(IdentityError::Empty);
    }
    if bytes.len() > OPAQUE_ID_MAX_BYTES {
        return Err(IdentityError::TooLong {
            len: bytes.len(),
            max: OPAQUE_ID_MAX_BYTES,
        });
    }
    if value.trim() != value {
        return Err(IdentityError::HasSurroundingWhitespace);
    }
    for ch in value.chars() {
        let code = u32::from(ch);
        let is_control = code <= 0x1F || (0x7F..=0x9F).contains(&code);
        if is_control || ch == '/' || ch == '\\' || ch == ':' {
            return Err(IdentityError::InvalidCharacter);
        }
    }
    Ok(())
}

/// Generates a transparent JSON-string newtype sharing the opaque identity validation rules.
macro_rules! opaque_identity_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
        #[serde(transparent)]
        #[ts(export_to = "plugin-protocol.ts", type = "string")]
        pub struct $name(String);

        impl $name {
            /// Constructs a validated value; rejects empty, overlong, control or path-separator input.
            pub fn try_new(value: String) -> Result<Self, IdentityError> {
                validate_opaque_id(&value).map(|_| Self(value))
            }

            /// Re-validates an already-deserialized value for contract enforcement.
            pub fn validate(&self) -> Result<(), IdentityError> {
                validate_opaque_id(&self.0)
            }

            /// Returns the underlying string value for serialization or logging.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

opaque_identity_newtype!(
    /// Plugin-issued installation identity, scoped to `(plugin content owner, provider, generation)`.
    ///
    /// Must not be routed across plugin content owner, provider, generation or session boundaries.
    AgentInstallationId
);
opaque_identity_newtype!(
    /// Plugin-issued conversation identity, scoped to its producing installation and generation.
    AgentConversationId
);
opaque_identity_newtype!(
    /// Plugin-issued turn identity for one completed agent turn.
    AgentTurnId
);
opaque_identity_newtype!(
    /// Opaque pagination cursor bound to `(provider, generation)`; never crosses a generation.
    AgentCursor
);
opaque_identity_newtype!(
    /// Opaque resource identity (skill, MCP server, etc.) within one provider.
    AgentResourceId
);
opaque_identity_newtype!(
    /// Identity of one tool call within a turn.
    AgentToolCallId
);
opaque_identity_newtype!(
    /// Host-issued, session-bound opaque project handle.
    ProjectHandle
);
opaque_identity_newtype!(
    /// Host-issued, session-bound opaque worktree handle.
    WorktreeHandle
);
opaque_identity_newtype!(
    /// Host-issued, per-generation CSPRNG session identity (§12.8).
    ///
    /// Generated by the Host once per runtime generation and never reused; the bootstrap must echo it
    /// exactly in the `$/initialize` result. It binds the pipe to a generation, not a publisher
    /// identity.
    SessionId
);

/// Maximum bytes for a single [`AgentProviderId`] (§5.2: 1..=63).
pub const AGENT_PROVIDER_ID_MAX_BYTES: usize = 63;

/// Validates the local provider-id grammar (§5.2): `^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$`.
fn validate_agent_provider_id(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(IdentityError::Empty);
    }
    if bytes.len() > AGENT_PROVIDER_ID_MAX_BYTES {
        return Err(IdentityError::TooLong {
            len: bytes.len(),
            max: AGENT_PROVIDER_ID_MAX_BYTES,
        });
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    {
        return Err(IdentityError::Grammar {
            reason: "agent provider id must be ASCII lowercase alphanumerics or '-' only",
        });
    }
    let first = bytes[0];
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return Err(IdentityError::Grammar {
            reason: "agent provider id must start with [a-z0-9]",
        });
    }
    let last = bytes[bytes.len() - 1];
    if last == b'-' {
        return Err(IdentityError::Grammar {
            reason: "agent provider id must not end with '-'",
        });
    }
    Ok(())
}

/// Local agent provider identity within one manifest (`claude-code`).
///
/// Distinct from the dotted [`PluginId`]; the global provider key is the structured pair
/// `(PluginId, AgentProviderId)`. Wire only carries the local id, and the activate descriptor must
/// equal the manifest contribution id exactly.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct AgentProviderId(String);

impl AgentProviderId {
    /// Constructs a validated local provider id.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_agent_provider_id(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_agent_provider_id(&self.0)
    }

    /// Returns the underlying local provider id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Windows reserved device names that must never become a path component or plugin id.
///
/// Kept lowercase because [`PluginId`] and its segments are always ASCII lowercase; the
/// comparison is therefore exact while still matching Windows' case-insensitive device rules.
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Returns true when the lowercase base name (before the first '.') is a reserved device name.
///
/// Because [`PluginId`] segments are dot-separated and contain no internal dots, checking each
/// segment and the full id catches both `nul` segments and full ids whose base before the first dot
/// is a device name (e.g. `nul.foo`).
fn is_reserved_device_name(value: &str) -> bool {
    let base = value.split('.').next().unwrap_or(value);
    RESERVED_DEVICE_NAMES.contains(&base)
}

/// Validates a [`PluginId`] per §5.2: dotted, ASCII lowercase, ≤128 bytes, no reserved device names
/// or trailing dot/space/colon/ADS syntax.
fn validate_plugin_id(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(IdentityError::Empty);
    }
    if bytes.len() > PLUGIN_ID_MAX_BYTES {
        return Err(IdentityError::TooLong {
            len: bytes.len(),
            max: PLUGIN_ID_MAX_BYTES,
        });
    }
    if bytes
        .iter()
        .any(|b| !b.is_ascii_lowercase() && !b.is_ascii_digit() && *b != b'-' && *b != b'.')
    {
        return Err(IdentityError::Grammar {
            reason: "plugin id must be ASCII lowercase alphanumerics, '-' or '.' only",
        });
    }
    if bytes.ends_with(b".") || bytes.ends_with(b" ") || bytes.ends_with(b":") {
        return Err(IdentityError::ReservedPathSyntax);
    }
    if bytes.contains(&b':') {
        return Err(IdentityError::ReservedPathSyntax);
    }
    let mut count = 0usize;
    for segment in value.split('.') {
        count += 1;
        if segment.is_empty() {
            return Err(IdentityError::Grammar {
                reason: "plugin id must not contain empty dot-separated segments",
            });
        }
        if segment.len() > 63 {
            return Err(IdentityError::Grammar {
                reason: "each plugin id segment must be 1..=63 bytes",
            });
        }
        let seg_bytes = segment.as_bytes();
        if !(seg_bytes[0].is_ascii_lowercase() || seg_bytes[0].is_ascii_digit()) {
            return Err(IdentityError::Grammar {
                reason: "each plugin id segment must start with [a-z0-9]",
            });
        }
        if is_reserved_device_name(segment) {
            return Err(IdentityError::ReservedDeviceName);
        }
    }
    if count < 2 {
        return Err(IdentityError::Grammar {
            reason: "plugin id must contain at least one dot",
        });
    }
    if is_reserved_device_name(value) {
        return Err(IdentityError::ReservedDeviceName);
    }
    Ok(())
}

/// Canonical, immutable plugin identity used for directory names, state keys and registry keys.
///
/// Identity comparison, directory names, state keys and registry keys all use the same canonical
/// value (§3.4). There is at most one installed instance per canonical id in the MVP.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct PluginId(String);

impl PluginId {
    /// Constructs a validated canonical plugin id.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_plugin_id(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_plugin_id(&self.0)
    }

    /// Returns the underlying canonical id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validates a [`AgentConfigurationKey`] (§13.1): 1..=512 ASCII bytes,
/// `^[A-Za-z0-9][A-Za-z0-9._-]{0,511}$`.
fn validate_agent_config_key(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(IdentityError::Empty);
    }
    if bytes.len() > AGENT_CONFIG_KEY_MAX_BYTES {
        return Err(IdentityError::TooLong {
            len: bytes.len(),
            max: AGENT_CONFIG_KEY_MAX_BYTES,
        });
    }
    if !bytes[0].is_ascii_alphanumeric() {
        return Err(IdentityError::Grammar {
            reason: "configuration key must start with [A-Za-z0-9]",
        });
    }
    if !bytes
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'.' || *b == b'_' || *b == b'-')
    {
        return Err(IdentityError::Grammar {
            reason: "configuration key must be [A-Za-z0-9._-] only",
        });
    }
    Ok(())
}

/// A configuration key surfaced by an agent provider, with its own 512-byte ASCII grammar.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct AgentConfigurationKey(String);

impl AgentConfigurationKey {
    /// Constructs a validated configuration key.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_agent_config_key(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_agent_config_key(&self.0)
    }

    /// Returns the underlying key string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validates a [`ClientRequestId`] (§13.1): canonical lowercase UUID `8-4-4-4-12` hex.
fn validate_client_request_id(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return Err(IdentityError::InvalidUuid);
    }
    let hyphens = [8usize, 13, 18, 23];
    for (index, &byte) in bytes.iter().enumerate() {
        if hyphens.contains(&index) {
            if byte != b'-' {
                return Err(IdentityError::InvalidUuid);
            }
            continue;
        }
        let is_lower_hex = byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte);
        if !is_lower_hex {
            return Err(IdentityError::InvalidUuid);
        }
    }
    Ok(())
}

/// Host-issued canonical lowercase UUID correlation id, used for end-to-end logging only.
///
/// Does not grant idempotency or automatic retry semantics.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct ClientRequestId(String);

impl ClientRequestId {
    /// Constructs a validated client request id from a canonical lowercase UUID string.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_client_request_id(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_client_request_id(&self.0)
    }

    /// Returns the underlying UUID string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// A JSON-RPC envelope id is carried as a raw string by the wire envelope module (§12.5); the two
// session-fatal `id: null` diagnostics are constructed as raw JSON by the runtime.

/// Validates a [`HostResolvedAbsolutePath`] (§13.1): 1..=32 KiB UTF-8, no NUL.
fn validate_host_resolved_path(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(IdentityError::Empty);
    }
    if bytes.len() > HOST_RESOLVED_PATH_MAX_BYTES {
        return Err(IdentityError::TooLong {
            len: bytes.len(),
            max: HOST_RESOLVED_PATH_MAX_BYTES,
        });
    }
    if bytes.contains(&0u8) {
        return Err(IdentityError::InvalidCharacter);
    }
    Ok(())
}

/// A Host-canonicalized absolute Windows path issued with a request.
///
/// The Host resolves and canonicalizes the working directory per request so plugins never need a
/// Plugin→Host path API. This is a call-context and audit fact, not a filesystem sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct HostResolvedAbsolutePath(String);

impl HostResolvedAbsolutePath {
    /// Constructs a validated Host-resolved absolute path.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_host_resolved_path(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_host_resolved_path(&self.0)
    }

    /// Returns the underlying path string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Validates an [`AgentPrompt`] (§13.1): 1..=1 MiB UTF-8, no NUL; whitespace and newlines preserved.
fn validate_agent_prompt(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(IdentityError::Empty);
    }
    if bytes.len() > AGENT_PROMPT_MAX_BYTES {
        return Err(IdentityError::TooLong {
            len: bytes.len(),
            max: AGENT_PROMPT_MAX_BYTES,
        });
    }
    if bytes.contains(&0u8) {
        return Err(IdentityError::InvalidCharacter);
    }
    Ok(())
}

/// An agent prompt: 1..=1 MiB UTF-8 with whitespace and newlines preserved (no trim/normalization).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct AgentPrompt(String);

impl AgentPrompt {
    /// Constructs a validated prompt.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_agent_prompt(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_agent_prompt(&self.0)
    }

    /// Returns the underlying prompt string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AgentPrompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Validates an [`Rfc3339Timestamp`] (§13.1): ≤64 ASCII bytes, strict RFC 3339 with Z or offset.
///
/// This is a structural check suitable for display data; it verifies the ASCII bound and the
/// `<date>T<time><offset>` skeleton with a trailing `Z` or numeric `+HH:MM`/`-HH:MM` offset.
fn validate_rfc3339(value: &str) -> Result<(), IdentityError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(IdentityError::Empty);
    }
    if bytes.len() > RFC3339_MAX_BYTES {
        return Err(IdentityError::TooLong {
            len: bytes.len(),
            max: RFC3339_MAX_BYTES,
        });
    }
    if !bytes.iter().all(u8::is_ascii) {
        return Err(IdentityError::InvalidRfc3339);
    }
    let date_end = value.find('T').ok_or(IdentityError::InvalidRfc3339)?;
    let date = &value[..date_end];
    if !is_valid_date(date) {
        return Err(IdentityError::InvalidRfc3339);
    }
    let remainder = &value[date_end + 1..];
    let (time, offset) = match remainder.strip_suffix('Z') {
        Some(time_without_zulu) => (time_without_zulu, Offset::Zulu),
        None if remainder.len() >= 6 => {
            let split = remainder.len() - 6;
            let candidate = &remainder[split..];
            if candidate.starts_with('+') || candidate.starts_with('-') {
                (&remainder[..split], Offset::Numeric(candidate))
            } else {
                return Err(IdentityError::InvalidRfc3339);
            }
        }
        None => return Err(IdentityError::InvalidRfc3339),
    };
    if !is_valid_time(time) {
        return Err(IdentityError::InvalidRfc3339);
    }
    if !is_valid_offset(offset) {
        return Err(IdentityError::InvalidRfc3339);
    }
    Ok(())
}

/// A parsed RFC 3339 offset, either `Z` or a signed `HH:MM` remainder.
enum Offset<'a> {
    Zulu,
    Numeric(&'a str),
}

/// Returns true when the offset is `Z` or a signed `HH:MM` of ASCII digits.
fn is_valid_offset(offset: Offset<'_>) -> bool {
    match offset {
        Offset::Zulu => true,
        Offset::Numeric(value) => {
            let bytes = value.as_bytes();
            bytes.len() == 6
                && (bytes[0] == b'+' || bytes[0] == b'-')
                && bytes[1..3].iter().all(u8::is_ascii_digit)
                && bytes[3] == b':'
                && bytes[4..6].iter().all(u8::is_ascii_digit)
        }
    }
}

/// Returns true when `date` is a `YYYY-MM-DD` skeleton of ASCII digits with the right separators.
fn is_valid_date(date: &str) -> bool {
    let bytes = date.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[0..4].iter().all(u8::is_ascii_digit)
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && bytes[8..10].iter().all(u8::is_ascii_digit)
}

/// Returns true when `time` is a `HH:MM:SS[.fraction]` skeleton of ASCII digits.
fn is_valid_time(time: &str) -> bool {
    if time.len() < 8 {
        return false;
    }
    let bytes = time.as_bytes();
    bytes[2] == b':'
        && bytes[5] == b':'
        && bytes[0..2].iter().all(u8::is_ascii_digit)
        && bytes[3..5].iter().all(u8::is_ascii_digit)
        && bytes[6..8].iter().all(u8::is_ascii_digit)
        && (time.len() == 8 || bytes[8] == b'.' || bytes[8] == b',')
}

/// A display timestamp validated as structural RFC 3339 with a `Z` or numeric offset.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct Rfc3339Timestamp(String);

impl Rfc3339Timestamp {
    /// Constructs a validated RFC 3339 timestamp.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_rfc3339(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_rfc3339(&self.0)
    }

    /// Returns the underlying timestamp string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A bounded page limit for paginated agent requests (§13.1: integer `1..=100`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, TS)]
#[ts(export_to = "plugin-protocol.ts", type = "number")]
pub struct AgentPageLimit(u8);

impl AgentPageLimit {
    /// Minimum and maximum inclusive page limit values.
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 100;

    /// Constructs a validated page limit.
    pub fn try_new(value: u8) -> Result<Self, IdentityError> {
        if !(Self::MIN..=Self::MAX).contains(&value) {
            return Err(IdentityError::NotInRange {
                lo: u64::from(Self::MIN),
                hi: u64::from(Self::MAX),
            });
        }
        Ok(Self(value))
    }

    /// Returns the raw limit value.
    pub fn get(self) -> u8 {
        self.0
    }
}

impl<'de> Deserialize<'de> for AgentPageLimit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        AgentPageLimit::try_new(value).map_err(serde::de::Error::custom)
    }
}

/// A JSON integer constrained to `0..=2^53-1` to avoid JavaScript precision loss (§13.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, TS)]
#[ts(export_to = "plugin-protocol.ts", type = "number")]
pub struct JsonSafeU64(u64);

impl JsonSafeU64 {
    /// Constructs a validated JSON-safe integer.
    pub fn try_new(value: u64) -> Result<Self, IdentityError> {
        if value > JSON_SAFE_U64_MAX {
            return Err(IdentityError::NotJsonSafeInteger);
        }
        Ok(Self(value))
    }

    /// Returns the raw value.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for JsonSafeU64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u64::deserialize(deserializer)?;
        JsonSafeU64::try_new(value).map_err(serde::de::Error::custom)
    }
}

impl FromStr for JsonSafeU64 {
    type Err = IdentityError;

    fn from_str(value: &str) -> Result<Self, IdentityError> {
        let parsed = value
            .parse::<u64>()
            .map_err(|_| IdentityError::NotJsonSafeInteger)?;
        JsonSafeU64::try_new(parsed)
    }
}

/// A finite JSON number; NaN and ±Infinity are rejected (§13.1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "plugin-protocol.ts", type = "number")]
pub struct FiniteJsonNumber(f64);

impl FiniteJsonNumber {
    /// Constructs a finite JSON number.
    pub fn try_new(value: f64) -> Result<Self, IdentityError> {
        if !value.is_finite() {
            return Err(IdentityError::NotFiniteNumber);
        }
        Ok(Self(value))
    }

    /// Returns the raw finite value.
    pub fn get(self) -> f64 {
        self.0
    }
}

/// Validates a [`ContentOwnerId`] (§6.1): `sha256-<64 lowercase hex>`.
fn validate_content_owner_id(value: &str) -> Result<(), IdentityError> {
    let Some(hex) = value.strip_prefix("sha256-") else {
        return Err(IdentityError::Grammar {
            reason: "content owner id must start with 'sha256-'",
        });
    };
    if hex.len() != 64 {
        return Err(IdentityError::Grammar {
            reason: "content owner id must be 'sha256-<64 lowercase hex>'",
        });
    }
    if !hex
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(IdentityError::Grammar {
            reason: "content owner id hex must be lowercase [0-9a-f]",
        });
    }
    Ok(())
}

/// A content-owner identity using the Windows-path-safe form `sha256-<64 lowercase hex>` (§6.1).
///
/// Display digests may be written as `sha256:<hex>`, but the colon form must never be used as a
/// directory name; this newtype is the path-safe form.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct ContentOwnerId(String);

impl ContentOwnerId {
    /// Constructs a validated content owner id.
    pub fn try_new(value: String) -> Result<Self, IdentityError> {
        validate_content_owner_id(&value).map(|_| Self(value))
    }

    /// Re-validates an already-deserialized value for contract enforcement.
    pub fn validate(&self) -> Result<(), IdentityError> {
        validate_content_owner_id(&self.0)
    }

    /// Returns the underlying content owner id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ContentOwnerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde::Deserialize;
    use serde_json::json;

    fn ok(value: &str) -> Result<(), IdentityError> {
        validate_plugin_id(value)
    }

    #[test]
    fn plugin_id_accepts_canonical_dotted_lowercase() {
        assert!(ok("ora.claude-code").is_ok());
        assert!(ok("ora.a.b.c").is_ok());
        assert!(ok("x.y").is_ok());
    }

    #[test]
    fn plugin_id_rejects_single_segment_and_uppercase_and_invalid_chars() {
        assert!(ok("claude-code").is_err());
        assert!(ok("ora.Claude").is_err());
        assert!(ok("ora.claude code").is_err());
        assert!(ok("ora./leading").is_err());
        assert!(ok(".ora.x").is_err());
        assert!(ok("ora..x").is_err());
    }

    #[test]
    fn plugin_id_rejects_device_names_and_reserved_path_syntax() {
        assert!(ok("ora.nul").is_err());
        assert!(ok("ora.con").is_err());
        assert!(ok("ora.com1").is_err());
        assert!(ok("ora.lpt9").is_err());
        assert!(ok("ora.x:ads").is_err());
        assert!(ok("ora.x.").is_err());
    }

    #[test]
    fn plugin_id_enforces_length_bounds() {
        let long_segment = "a".repeat(64);
        assert!(ok(&format!("ora.{long_segment}")).is_err());
        let huge = format!("ora.{}", "a".repeat(130));
        assert!(ok(&huge).is_err());
    }

    #[test]
    fn agent_provider_id_grammar_boundaries() {
        assert!(validate_agent_provider_id("claude-code").is_ok());
        assert!(validate_agent_provider_id("a").is_ok());
        assert!(validate_agent_provider_id("a.b").is_err());
        assert!(validate_agent_provider_id("-a").is_err());
        assert!(validate_agent_provider_id("a-").is_err());
        assert!(validate_agent_provider_id("Claude").is_err());
        assert!(validate_agent_provider_id(&"a".repeat(64)).is_err());
        assert!(validate_agent_provider_id(&"a".repeat(63)).is_ok());
    }

    #[test]
    fn opaque_ids_reject_control_and_separators_and_trim() {
        assert!(validate_opaque_id("abc").is_ok());
        assert!(validate_opaque_id(" abc").is_err());
        assert!(validate_opaque_id("a/b").is_err());
        assert!(validate_opaque_id("a\\b").is_err());
        assert!(validate_opaque_id("a:b").is_err());
        assert!(validate_opaque_id("a\tb").is_err());
        assert!(validate_opaque_id(&"a".repeat(257)).is_err());
        assert!(validate_opaque_id(&"a".repeat(256)).is_ok());
    }

    #[test]
    fn agent_prompt_allows_newlines_rejects_nul_and_size() {
        assert!(validate_agent_prompt("hello\nworld").is_ok());
        assert!(validate_agent_prompt("line1\nline2 /with \\paths: ok").is_ok());
        assert!(validate_agent_prompt("a\0b").is_err());
        assert!(validate_agent_prompt("").is_err());
        assert!(validate_agent_prompt(&"a".repeat(AGENT_PROMPT_MAX_BYTES)).is_ok());
        assert!(validate_agent_prompt(&"a".repeat(AGENT_PROMPT_MAX_BYTES + 1)).is_err());
    }

    #[test]
    fn agent_config_key_grammar() {
        assert!(validate_agent_config_key("ANTHROPIC_API_KEY").is_ok());
        assert!(validate_agent_config_key("a.b_c-d").is_ok());
        assert!(validate_agent_config_key(".leading").is_err());
        assert!(validate_agent_config_key(&"a".repeat(513)).is_err());
        assert!(validate_agent_config_key(&"a".repeat(512)).is_ok());
    }

    #[test]
    fn client_request_id_uuid_grammar() {
        assert!(validate_client_request_id("123e4567-e89b-12d3-a456-426614174000").is_ok());
        assert!(validate_client_request_id("123E4567-e89b-12d3-a456-426614174000").is_err());
        assert!(validate_client_request_id("not-a-uuid").is_err());
        assert!(validate_client_request_id("123e4567-e89b-12d3-a456-42661417400").is_err());
    }

    #[test]
    fn json_safe_u64_boundaries() {
        assert!(JsonSafeU64::try_new(0).is_ok());
        assert!(JsonSafeU64::try_new(JSON_SAFE_U64_MAX).is_ok());
        assert!(JsonSafeU64::try_new(JSON_SAFE_U64_MAX + 1).is_err());
    }

    #[test]
    fn json_safe_u64_rejects_overflow_on_deserialize() {
        let value = json!({ "n": JSON_SAFE_U64_MAX + 1 });
        let result: Result<TestWrap, _> = serde_json::from_value(value);
        assert!(result.is_err());
    }

    #[derive(Deserialize)]
    struct TestWrap {
        #[allow(dead_code)]
        n: JsonSafeU64,
    }

    #[test]
    fn agent_page_limit_boundaries() {
        assert!(AgentPageLimit::try_new(0).is_err());
        assert!(AgentPageLimit::try_new(1).is_ok());
        assert!(AgentPageLimit::try_new(100).is_ok());
        assert!(AgentPageLimit::try_new(101).is_err());
    }

    #[test]
    fn content_owner_id_grammar() {
        assert!(validate_content_owner_id(&format!("sha256-{}", "a".repeat(64))).is_ok());
        assert!(validate_content_owner_id("sha256-abc").is_err());
        assert!(validate_content_owner_id("sha256:abcd").is_err());
        assert!(validate_content_owner_id(&format!("sha256-{}", "G".repeat(64))).is_err());
    }

    #[test]
    fn rfc3339_validation() {
        assert!(validate_rfc3339("2026-07-16T12:34:56Z").is_ok());
        assert!(validate_rfc3339("2026-07-16T12:34:56.123+08:00").is_ok());
        assert!(validate_rfc3339("not-a-date").is_err());
        assert!(validate_rfc3339("2026-07-16 12:34:56Z").is_err());
    }

    #[test]
    fn newtypes_serialize_as_transparent_primitives() {
        let id = AgentInstallationId::try_new("abc".to_string())
            .unwrap_or_else(|error| panic!("valid id: {error}"));
        let value = serde_json::to_value(&id).unwrap_or_else(|error| panic!("serialize: {error}"));
        assert_eq!(value, json!("abc"));

        let prompt = AgentPrompt::try_new("hello\nworld".to_string())
            .unwrap_or_else(|error| panic!("valid prompt: {error}"));
        let value =
            serde_json::to_value(&prompt).unwrap_or_else(|error| panic!("serialize: {error}"));
        assert_eq!(value, json!("hello\nworld"));

        let limit =
            AgentPageLimit::try_new(50).unwrap_or_else(|error| panic!("valid limit: {error}"));
        let value =
            serde_json::to_value(limit).unwrap_or_else(|error| panic!("serialize: {error}"));
        assert_eq!(value, json!(50));

        let n = JsonSafeU64::try_new(42).unwrap_or_else(|error| panic!("valid number: {error}"));
        let value = serde_json::to_value(n).unwrap_or_else(|error| panic!("serialize: {error}"));
        assert_eq!(value, json!(42));
    }
}
