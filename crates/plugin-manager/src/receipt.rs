//! Installation receipt (design-v3 §6.2).
//!
//! `.ora/receipt.json` is Host-generated in staging and committed atomically with the plugin
//! directory. It does NOT belong to the plugin content digest; a `.ora/` in the source tree is
//! rejected so authors cannot forge a receipt. Scan and start both verify the receipt's
//! identity/version match the actual manifest.

use ora_plugin_protocol::{JsonSafeU64, PluginId, PluginVersion};
use serde::{Deserialize, Serialize};

use crate::state::{ContentDigest, OperationId};

/// The only supported receipt schema version (§6.2).
pub const RECEIPT_VERSION_V1: u32 = 1;

/// The install source (§6.2). MVP supports only an authorized local directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReceiptSource {
    LocalDirectory,
}

/// The Host-generated installation receipt (§6.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Receipt {
    pub receipt_version: u32,
    pub plugin_id: PluginId,
    pub plugin_version: PluginVersion,
    pub source: ReceiptSource,
    pub installed_at_unix_ms: JsonSafeU64,
    pub content_digest: ContentDigest,
    pub file_count: u32,
    pub total_bytes: JsonSafeU64,
    pub operation_id: OperationId,
}

impl Receipt {
    /// Constructs a receipt at the current schema version (§6.2).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        plugin_id: PluginId,
        plugin_version: PluginVersion,
        source: ReceiptSource,
        installed_at_unix_ms: JsonSafeU64,
        content_digest: ContentDigest,
        file_count: u32,
        total_bytes: JsonSafeU64,
        operation_id: OperationId,
    ) -> Self {
        Self {
            receipt_version: RECEIPT_VERSION_V1,
            plugin_id,
            plugin_version,
            source,
            installed_at_unix_ms,
            content_digest,
            file_count,
            total_bytes,
            operation_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn receipt_projects_camelcase_and_round_trips() {
        let receipt = Receipt::new(
            PluginId::try_new("ora.claude-code".to_string()).unwrap_or_else(|e| panic!("pid: {e}")),
            PluginVersion::try_new("0.1.0".to_string()).unwrap_or_else(|e| panic!("ver: {e}")),
            ReceiptSource::LocalDirectory,
            JsonSafeU64::try_new(1_784_170_000_000u64).unwrap_or_else(|e| panic!("ms: {e}")),
            ContentDigest::try_new(format!("sha256:{}", "a".repeat(64)))
                .unwrap_or_else(|e| panic!("digest: {e}")),
            42,
            JsonSafeU64::try_new(1_234_567u64).unwrap_or_else(|e| panic!("bytes: {e}")),
            OperationId::try_new("op-1".to_string()).unwrap_or_else(|e| panic!("op: {e}")),
        );
        let value = serde_json::to_value(&receipt).unwrap_or_else(|e| panic!("serialize: {e}"));
        assert_eq!(value["receiptVersion"], json!(1));
        assert_eq!(value["pluginId"], json!("ora.claude-code"));
        assert_eq!(value["source"], json!("localDirectory"));
        assert_eq!(value["installedAtUnixMs"], json!(1_784_170_000_000u64));
        assert_eq!(
            value["contentDigest"],
            json!(format!("sha256:{}", "a".repeat(64)))
        );
        assert_eq!(value["fileCount"], json!(42));
        assert_eq!(value["totalBytes"], json!(1_234_567u64));
        assert_eq!(value["operationId"], json!("op-1"));

        // Round-trip + unknown-field rejection.
        let parsed: Receipt =
            serde_json::from_value(value).unwrap_or_else(|e| panic!("deserialize: {e}"));
        assert_eq!(parsed.receipt_version, RECEIPT_VERSION_V1);
        let mut with_extra =
            serde_json::to_value(&parsed).unwrap_or_else(|e| panic!("serialize: {e}"));
        if let serde_json::Value::Object(ref mut map) = with_extra {
            map.insert("rogue".to_string(), json!("nope"));
        }
        assert!(serde_json::from_value::<Receipt>(with_extra).is_err());
    }
}
