//! Wire Frame v1 codec (design-v3 §12.2–§12.4, §12.9).
//!
//! Frame layout on the stdio byte stream:
//!
//! ```text
//! offset  size  field
//! 0       4     length: signed i32, big-endian (payload bytes only)
//! 4       1     type:   signed i8
//! 5       N     payload: exactly `length` bytes of UTF-8 JSON
//! ```
//!
//! `HEADER_LEN == 5` and `length` counts payload bytes only (never the header or type byte).
//! The encoder and decoder share identical validation so the encoder can never emit a frame the
//! decoder would reject. This module is the pure wire format; the async incremental reader/writer
//! lives in the plugin-manager transport layer and reuses [`parse_header`] and [`FrameType`].

use std::collections::VecDeque;

use thiserror::Error;

/// Fixed frame header length in bytes: 4-byte big-endian length + 1-byte type.
pub const HEADER_LEN: usize = 5;

/// Maximum payload bytes for a single frame (§12.2: MVP `MAX_PAYLOAD_BYTES = 8 MiB`).
pub const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

/// Stable frame type codes (§12.2). `0`, negative values and `4..=127` are reserved; any reserved
/// value is a fatal [`FrameError::UnknownFrameType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i8)]
pub enum FrameType {
    /// JSON-RPC request envelope.
    Request = 1,
    /// JSON-RPC success/error response envelope.
    Response = 2,
    /// JSON-RPC notification envelope.
    Notification = 3,
}

impl FrameType {
    /// Returns the stable signed wire code for this frame type.
    pub const fn to_i8(self) -> i8 {
        self as i8
    }

    /// Parses a signed wire code into a frame type, rejecting reserved values.
    pub fn from_i8(value: i8) -> Result<Self, FrameError> {
        match value {
            1 => Ok(Self::Request),
            2 => Ok(Self::Response),
            3 => Ok(Self::Notification),
            other => Err(FrameError::UnknownFrameType { frame_type: other }),
        }
    }
}

/// Errors produced while encoding or decoding a single wire frame.
///
/// These are the wire-level failures that terminate the current connection per §12.4; none of them
/// attempt byte resynchronization.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FrameError {
    #[error("frame length must be > 0 (got {length})")]
    ZeroOrNegativeLength { length: i32 },
    #[error("frame payload exceeds {max} bytes (got {len})")]
    PayloadTooLarge { len: usize, max: usize },
    #[error("unknown frame type {frame_type}")]
    UnknownFrameType { frame_type: i8 },
    #[error("frame payload must not be empty")]
    EmptyPayload,
    #[error("unexpected end of stream before a complete frame header")]
    UnexpectedEof,
    #[error("incomplete frame: declared {declared} payload bytes but only {available} available")]
    IncompleteFrame { declared: usize, available: usize },
}

/// Validates a decoded signed length against the §12.2 rules: reject `<= 0`, then reject
/// `> MAX_PAYLOAD_BYTES`, then convert to `usize`.
pub fn parse_length(raw: i32) -> Result<usize, FrameError> {
    if raw <= 0 {
        return Err(FrameError::ZeroOrNegativeLength { length: raw });
    }
    let length = usize::try_from(raw).map_err(|_| FrameError::PayloadTooLarge {
        len: raw as usize,
        max: MAX_PAYLOAD_BYTES,
    })?;
    if length > MAX_PAYLOAD_BYTES {
        return Err(FrameError::PayloadTooLarge {
            len: length,
            max: MAX_PAYLOAD_BYTES,
        });
    }
    Ok(length)
}

/// Parses a 5-byte frame header into `(payload_length, frame_type)`.
///
/// Length is validated before any allocation, and the type is rejected before any payload read,
/// matching §3.17 (length validated pre-allocation) and §12.4 (unknown type fails immediately).
pub fn parse_header(header: &[u8; HEADER_LEN]) -> Result<(usize, FrameType), FrameError> {
    let raw_length = i32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    let length = parse_length(raw_length)?;
    let frame_type = FrameType::from_i8(i8::from_be_bytes([header[4]]))?;
    Ok((length, frame_type))
}

/// Encodes one complete frame as `header || payload`.
///
/// The encoder rejects empty payloads, oversized payloads and (via [`FrameType`]) unknown types,
/// so it can never emit a frame the decoder would reject (§12.2).
pub fn encode_frame(frame_type: FrameType, payload: &[u8]) -> Result<Vec<u8>, FrameError> {
    if payload.is_empty() {
        return Err(FrameError::EmptyPayload);
    }
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(FrameError::PayloadTooLarge {
            len: payload.len(),
            max: MAX_PAYLOAD_BYTES,
        });
    }
    let length_i32 = i32::try_from(payload.len()).map_err(|_| FrameError::PayloadTooLarge {
        len: payload.len(),
        max: MAX_PAYLOAD_BYTES,
    })?;
    let mut frame = Vec::with_capacity(HEADER_LEN + payload.len());
    frame.extend_from_slice(&length_i32.to_be_bytes());
    frame.push(frame_type.to_i8() as u8);
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Decodes one complete frame from a byte slice.
///
/// Returns the frame type and a slice over exactly the declared payload bytes. Incomplete input
/// (fewer than [`HEADER_LEN`] bytes, or fewer payload bytes than declared) is reported via
/// [`FrameError::UnexpectedEof`] / [`FrameError::IncompleteFrame`]; the incremental
/// [`FrameDecoder`] distinguishes "need more bytes" for streaming use.
pub fn decode_frame(bytes: &[u8]) -> Result<(FrameType, &[u8]), FrameError> {
    let header: &[u8; HEADER_LEN] = bytes
        .get(..HEADER_LEN)
        .ok_or(FrameError::UnexpectedEof)?
        .try_into()
        .map_err(|_| FrameError::UnexpectedEof)?;
    let (length, frame_type) = parse_header(header)?;
    let available = bytes.len() - HEADER_LEN;
    if available < length {
        return Err(FrameError::IncompleteFrame {
            declared: length,
            available,
        });
    }
    Ok((frame_type, &bytes[HEADER_LEN..HEADER_LEN + length]))
}

/// Incremental, allocation-bounded frame decoder for a byte stream (§12.4).
///
/// Bytes are pushed in arbitrary chunks; [`FrameDecoder::next_frame`] emits each complete frame as
/// it becomes available and returns `Ok(None)` when more bytes are needed. The internal buffer never
/// exceeds `HEADER_LEN + MAX_PAYLOAD_BYTES` for a valid in-progress frame because a declared length
/// above the cap is rejected before buffering the payload. UTF-8/JSON/envelope validation happens in
/// a later layer; this decoder only produces raw payload bytes and a frame type.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: VecDeque<u8>,
}

impl FrameDecoder {
    /// Creates an empty decoder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends bytes to the decoder's internal buffer.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend(bytes);
    }

    /// Attempts to decode one complete frame, draining its bytes on success.
    ///
    /// Returns `Ok(None)` when no complete frame is available yet. A returned error is fatal for
    /// the connection per §12.4 and leaves the decoder in an unspecified state.
    pub fn next_frame(&mut self) -> Result<Option<(FrameType, Vec<u8>)>, FrameError> {
        if self.buffer.len() < HEADER_LEN {
            return Ok(None);
        }
        let mut header = [0u8; HEADER_LEN];
        for (index, slot) in header.iter_mut().enumerate() {
            *slot = self.buffer[index];
        }
        let (length, frame_type) = parse_header(&header)?;
        if self.buffer.len() < HEADER_LEN + length {
            return Ok(None);
        }
        self.buffer.drain(..HEADER_LEN);
        let payload: Vec<u8> = self.buffer.drain(..length).collect();
        Ok(Some((frame_type, payload)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn hex_to_bytes(hex: &str) -> Vec<u8> {
        let cleaned: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        assert_eq!(
            cleaned.len() % 2,
            0,
            "hex string must have an even number of digits: {cleaned}"
        );
        (0..cleaned.len())
            .step_by(2)
            .map(|index| {
                u8::from_str_radix(&cleaned[index..index + 2], 16).unwrap_or_else(|error| {
                    panic!("invalid hex byte `{}`: {error}", &cleaned[index..index + 2])
                })
            })
            .collect()
    }

    /// A golden vector from §12.9: payload text, declared length, header hex, frame type.
    struct Golden {
        payload: &'static str,
        frame_type: FrameType,
        header_hex: &'static str,
    }

    impl Golden {
        fn expected_frame(&self) -> Vec<u8> {
            let mut bytes = hex_to_bytes(self.header_hex);
            bytes.extend_from_slice(self.payload.as_bytes());
            bytes
        }
    }

    const GOLDEN: &[Golden] = &[
        Golden {
            payload: r#"{"jsonrpc":"2.0","id":"h:1","method":"ping","params":{}}"#,
            frame_type: FrameType::Request,
            header_hex: "00 00 00 38 01",
        },
        Golden {
            payload: r#"{"jsonrpc":"2.0","id":"h:1","result":"ok"}"#,
            frame_type: FrameType::Response,
            header_hex: "00 00 00 2a 02",
        },
        Golden {
            payload: r#"{"jsonrpc":"2.0","method":"$/exit"}"#,
            frame_type: FrameType::Notification,
            header_hex: "00 00 00 23 03",
        },
        Golden {
            payload: r#"{"jsonrpc":"2.0","method":"$/stream","params":{"id":"h:1","seq":1,"value":{"kind":"textDelta","text":"你好"}}}"#,
            frame_type: FrameType::Notification,
            header_hex: "00 00 00 70 03",
        },
    ];

    #[test]
    fn golden_vectors_round_trip_and_match_declared_lengths() {
        for vector in GOLDEN {
            let expected = vector.expected_frame();
            let encoded = encode_frame(vector.frame_type, vector.payload.as_bytes())
                .unwrap_or_else(|error| panic!("encode failed: {error}"));
            assert_eq!(encoded, expected);

            let (frame_type, payload) =
                decode_frame(&encoded).unwrap_or_else(|error| panic!("decode failed: {error}"));
            assert_eq!(frame_type, vector.frame_type);
            assert_eq!(payload, vector.payload.as_bytes());

            // The design declares these exact byte counts in §12.9.
            let header_length = usize::try_from(i32::from_be_bytes(
                encoded[0..4]
                    .try_into()
                    .unwrap_or_else(|error| panic!("header slice error: {error}")),
            ))
            .unwrap_or_else(|error| panic!("length conversion failed: {error}"));
            assert_eq!(header_length, vector.payload.len());
        }
    }

    #[test]
    fn illegal_vectors_fail_with_expected_errors() {
        assert_eq!(
            decode_frame(&hex_to_bytes("00 00 00 00 01")),
            Err(FrameError::ZeroOrNegativeLength { length: 0 })
        );
        assert_eq!(
            decode_frame(&hex_to_bytes("ff ff ff ff 01")),
            Err(FrameError::ZeroOrNegativeLength { length: -1 })
        );
        assert_eq!(
            decode_frame(&hex_to_bytes("00 80 00 01 01")),
            Err(FrameError::PayloadTooLarge {
                len: 8_388_609,
                max: MAX_PAYLOAD_BYTES,
            })
        );
        assert_eq!(
            decode_frame(&hex_to_bytes("00 00 00 02 7f 7b 7d")),
            Err(FrameError::UnknownFrameType { frame_type: 127 })
        );
    }

    #[test]
    fn encoder_rejects_empty_oversized_and_unknown() {
        assert_eq!(
            encode_frame(FrameType::Request, b""),
            Err(FrameError::EmptyPayload)
        );
        let oversized = vec![b'a'; MAX_PAYLOAD_BYTES + 1];
        assert_eq!(
            encode_frame(FrameType::Request, &oversized),
            Err(FrameError::PayloadTooLarge {
                len: MAX_PAYLOAD_BYTES + 1,
                max: MAX_PAYLOAD_BYTES,
            })
        );
    }

    #[test]
    fn max_payload_boundary_round_trips() {
        let payload = vec![b'x'; MAX_PAYLOAD_BYTES];
        let encoded = encode_frame(FrameType::Response, &payload)
            .unwrap_or_else(|error| panic!("max encode failed: {error}"));
        let (frame_type, decoded_payload) =
            decode_frame(&encoded).unwrap_or_else(|error| panic!("max decode failed: {error}"));
        assert_eq!(frame_type, FrameType::Response);
        assert_eq!(decoded_payload, payload);
    }

    #[test]
    fn decoder_rejects_partial_header_and_partial_payload() {
        assert_eq!(
            decode_frame(b"\x00\x00\x00"),
            Err(FrameError::UnexpectedEof)
        );
        let mut partial = encode_frame(FrameType::Notification, br#"{"a":1}"#)
            .unwrap_or_else(|error| panic!("encode failed: {error}"));
        partial.truncate(HEADER_LEN + 1);
        assert_eq!(
            decode_frame(&partial),
            Err(FrameError::IncompleteFrame {
                declared: 7,
                available: 1,
            })
        );
    }

    #[test]
    fn incremental_decoder_handles_byte_chunks_and_coalesced_frames() {
        let first = encode_frame(FrameType::Request, br#"{"i":1}"#)
            .unwrap_or_else(|error| panic!("encode failed: {error}"));
        let second = encode_frame(FrameType::Response, br#"{"i":2}"#)
            .unwrap_or_else(|error| panic!("encode failed: {error}"));
        let mut stream = Vec::new();
        stream.extend_from_slice(&first);
        stream.extend_from_slice(&second);

        let mut decoder = FrameDecoder::new();
        // Feed one byte at a time; only complete frames should surface.
        let mut emitted = Vec::new();
        for byte in &stream {
            decoder.push(std::slice::from_ref(byte));
            while let Some((frame_type, payload)) = decoder
                .next_frame()
                .unwrap_or_else(|error| panic!("decoder error: {error}"))
            {
                emitted.push((frame_type, payload));
            }
        }
        assert_eq!(emitted.len(), 2);
        assert_eq!(emitted[0].0, FrameType::Request);
        assert_eq!(emitted[0].1, br#"{"i":1}"#);
        assert_eq!(emitted[1].0, FrameType::Response);
        assert_eq!(emitted[1].1, br#"{"i":2}"#);
    }

    #[test]
    fn incremental_decoder_reports_partial_without_bytes_then_completes() {
        let frame = encode_frame(FrameType::Notification, br#"{"x":42}"#)
            .unwrap_or_else(|error| panic!("encode failed: {error}"));
        let mut decoder = FrameDecoder::new();
        decoder.push(&frame[..3]);
        assert_eq!(decoder.next_frame(), Ok(None));
        decoder.push(&frame[3..]);
        let (frame_type, payload) = decoder
            .next_frame()
            .unwrap_or_else(|error| panic!("decoder error: {error}"))
            .unwrap_or_else(|| panic!("expected complete frame after feeding remainder"));
        assert_eq!(frame_type, FrameType::Notification);
        assert_eq!(payload, br#"{"x":42}"#);
    }

    #[test]
    fn incremental_decoder_rejects_oversized_length_before_buffering_payload() {
        let mut decoder = FrameDecoder::new();
        // header declaring 8 MiB + 1, type Request
        decoder.push(&hex_to_bytes("00 80 00 01 01"));
        assert_eq!(
            decoder.next_frame(),
            Err(FrameError::PayloadTooLarge {
                len: 8_388_609,
                max: MAX_PAYLOAD_BYTES,
            })
        );
    }
}
