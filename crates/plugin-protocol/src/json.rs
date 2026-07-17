//! Strict JSON parsing with duplicate-key detection and depth limiting
//! (design-v3 §5.5, §8.2, §12.5).
//!
//! `serde_json` accepts duplicate object keys (last wins) and uses a default 128-deep recursion
//! limit. Ora must reject duplicate keys and cap nesting at 64 BEFORE typed schema validation, so
//! this module drives `serde_json`'s deserializer with a custom visitor. The visitor builds a
//! `serde_json::Value` while detecting duplicate keys and enforcing the depth bound; the specific
//! violation is preserved via a thread-local because the visitor's error type is fixed to
//! `serde_json::Error` by the deserializer.

use std::cell::RefCell;
use std::fmt;

use serde::de::{self, DeserializeSeed, Deserializer as _, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
use thiserror::Error;

/// Default maximum JSON nesting depth (§5.5: 64).
pub const DEFAULT_MAX_DEPTH: u32 = 64;

/// Errors produced by strict JSON parsing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StrictJsonError {
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("duplicate object key: {key}")]
    DuplicateKey { key: String },
    #[error("JSON nesting depth exceeds {max}")]
    DepthExceeded { max: u32 },
    #[error("JSON value must be a top-level object")]
    NotAnObject,
}

thread_local! {
    /// The first strict violation observed by the visitor, if any. Read by [`parse_strict`] when the
    /// deserializer returns an error, so the specific variant can be reported instead of a generic
    /// JSON error.
    static STRICT_VIOLATION: RefCell<Option<StrictJsonError>> = const { RefCell::new(None) };
}

/// Records the first strict violation, if no earlier one was recorded.
fn record_violation(error: StrictJsonError) {
    STRICT_VIOLATION.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(error);
        }
    });
}

/// Parses a JSON document with duplicate-key detection and depth limiting.
///
/// Trailing non-whitespace data is rejected by `Deserializer::end`, matching the requirement that a
/// frame payload is exactly one top-level JSON value.
pub fn parse_strict(input: &[u8], max_depth: u32) -> Result<Value, StrictJsonError> {
    STRICT_VIOLATION.with(|cell| {
        *cell.borrow_mut() = None;
    });
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = deserializer
        .deserialize_any(StrictVisitor {
            depth: 0,
            max_depth,
        })
        .map_err(|error| {
            STRICT_VIOLATION
                .with(|cell| cell.borrow_mut().take())
                .unwrap_or_else(|| StrictJsonError::InvalidJson(error.to_string()))
        })?;
    deserializer
        .end()
        .map_err(|error| StrictJsonError::InvalidJson(error.to_string()))?;
    Ok(value)
}

/// Parses a JSON document and requires the top-level value to be an object (§12.5).
pub fn parse_strict_object(
    input: &[u8],
    max_depth: u32,
) -> Result<Map<String, Value>, StrictJsonError> {
    match parse_strict(input, max_depth)? {
        Value::Object(object) => Ok(object),
        _ => Err(StrictJsonError::NotAnObject),
    }
}

#[derive(Clone, Copy)]
struct DepthSeed {
    depth: u32,
    max_depth: u32,
}

impl<'de> DeserializeSeed<'de> for DepthSeed {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Value, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictVisitor {
            depth: self.depth,
            max_depth: self.max_depth,
        })
    }
}

struct StrictVisitor {
    depth: u32,
    max_depth: u32,
}

impl StrictVisitor {
    /// Returns `Err` when entering a container would exceed the depth bound.
    fn enter<E: de::Error>(&self) -> Result<(), E> {
        if self.depth + 1 > self.max_depth {
            record_violation(StrictJsonError::DepthExceeded {
                max: self.max_depth,
            });
            return Err(E::custom("ora strict json: nesting depth exceeded"));
        }
        Ok(())
    }
}

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::from(value))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::from(value))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::from(value))
    }

    fn visit_str<E>(self, value: &str) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_unit<E>(self) -> Result<Value, E>
    where
        E: de::Error,
    {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        self.enter()?;
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element_seed(DepthSeed {
            depth: self.depth + 1,
            max_depth: self.max_depth,
        })? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        self.enter()?;
        let mut object = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if object.contains_key(&key) {
                record_violation(StrictJsonError::DuplicateKey { key });
                return Err(de::Error::custom("ora strict json: duplicate key"));
            }
            let value = map.next_value_seed(DepthSeed {
                depth: self.depth + 1,
                max_depth: self.max_depth,
            })?;
            object.insert(key, value);
        }
        Ok(Value::Object(object))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn parses_simple_object() {
        let value = parse_strict(br#"{"a":1,"b":[2,3]}"#, DEFAULT_MAX_DEPTH)
            .unwrap_or_else(|error| panic!("parse failed: {error}"));
        assert_eq!(value, json!({"a": 1, "b": [2, 3]}));
    }

    #[test]
    fn rejects_duplicate_keys() {
        let result = parse_strict(br#"{"a":1,"a":2}"#, DEFAULT_MAX_DEPTH);
        assert_eq!(
            result,
            Err(StrictJsonError::DuplicateKey {
                key: "a".to_string()
            })
        );
    }

    #[test]
    fn rejects_nested_duplicate_keys() {
        let result = parse_strict(br#"{"outer":{"x":1,"x":2}}"#, DEFAULT_MAX_DEPTH);
        assert_eq!(
            result,
            Err(StrictJsonError::DuplicateKey {
                key: "x".to_string()
            })
        );
    }

    #[test]
    fn depth_64_allowed_and_65_rejected() {
        let mut inner = String::from("1");
        for _ in 0..64 {
            inner = format!("[{inner}]");
        }
        // 64 nested arrays: depth 64 is allowed.
        assert!(parse_strict(inner.as_bytes(), DEFAULT_MAX_DEPTH).is_ok());

        let too_deep = format!("[{inner}]"); // depth 65
        assert_eq!(
            parse_strict(too_deep.as_bytes(), DEFAULT_MAX_DEPTH),
            Err(StrictJsonError::DepthExceeded {
                max: DEFAULT_MAX_DEPTH
            })
        );
    }

    #[test]
    fn rejects_trailing_data_and_non_object() {
        assert!(parse_strict(br#"{"a":1} extra"#, DEFAULT_MAX_DEPTH).is_err());
        assert_eq!(
            parse_strict_object(br#"[1,2,3]"#, DEFAULT_MAX_DEPTH),
            Err(StrictJsonError::NotAnObject)
        );
    }

    #[test]
    fn rejects_invalid_json() {
        assert!(matches!(
            parse_strict(b"{not json", DEFAULT_MAX_DEPTH),
            Err(StrictJsonError::InvalidJson(_))
        ));
    }
}
