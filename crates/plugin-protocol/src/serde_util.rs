//! Strict `Option<T>` deserialization that rejects explicit JSON `null` (§13.1, §12.5).
//!
//! serde's default `Option<T>` deserialization accepts a present `null` as `None`. Ora requires
//! that an `Option` field may be omitted, but an explicit `null` is a contract violation. Apply
//! this as `#[serde(default, deserialize_with = "strict_option")] #[ts(optional)]` on each
//! `Option<T>` field so the Rust type rejects `null` on the wire and the generated TypeScript
//! emits `field?: T` (omittable, not nullable).

use std::fmt;
use std::marker::PhantomData;

use serde::de::{self, Deserialize, Deserializer, Visitor};

/// Deserializes an `Option<T>` that accepts omission but rejects an explicit `null`.
pub fn strict_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_option(StrictOptionVisitor::<T>(PhantomData))
}

struct StrictOptionVisitor<T>(PhantomData<T>);

impl<'de, T> Visitor<'de> for StrictOptionVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Option<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an omitted field or a present value, but not explicit null")
    }

    fn visit_none<E>(self) -> Result<Option<T>, E>
    where
        E: de::Error,
    {
        Err(de::Error::custom(
            "explicit null is not allowed; omit the field instead",
        ))
    }

    fn visit_unit<E>(self) -> Result<Option<T>, E>
    where
        E: de::Error,
    {
        Err(de::Error::custom(
            "explicit null is not allowed; omit the field instead",
        ))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Option<T>, D::Error>
    where
        D: Deserializer<'de>,
    {
        T::deserialize(deserializer).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, PartialEq, Deserialize)]
    struct Wrap {
        #[serde(default, deserialize_with = "strict_option")]
        field: Option<String>,
    }

    #[test]
    fn accepts_omitted_and_present_rejects_null() {
        assert_eq!(
            serde_json::from_value::<Wrap>(json!({})).unwrap_or_else(|e| panic!("omitted: {e}")),
            Wrap { field: None }
        );
        assert_eq!(
            serde_json::from_value::<Wrap>(json!({"field": "x"}))
                .unwrap_or_else(|e| panic!("present: {e}")),
            Wrap {
                field: Some("x".to_string())
            }
        );
        assert!(
            serde_json::from_value::<Wrap>(json!({"field": null})).is_err(),
            "explicit null must be rejected"
        );
    }
}
