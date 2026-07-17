//! Plugin manifest v1 (design-v3 §5).
//!
//! The manifest lives at `package.json` under the top-level `ora` object. Ora's schema is a strict
//! field set that rejects unknown fields; the parser reads the minimal envelope
//! (`manifestVersion`, `id`, `kind`) and routes to the matching version's schema (§5.5). The
//! `kind` field discriminates a [`PluginKindManifest`] union whose variant data (`main`,
//! `contributes`) are siblings of `kind` in the JSON — modelled here with a Rust enum and parsed
//! manually so unknown fields are rejected at the top level without serde's flatten limitation.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;
use ts_rs::TS;

use crate::identity::{AgentProviderId, IdentityError, PluginId};
use crate::json::parse_strict_object;
use crate::serde_util::strict_option;

/// The only supported manifest schema version (§5.1).
pub const MANIFEST_VERSION_V1: u32 = 1;

/// Maximum contribution count per manifest (§5.5).
pub const MAX_CONTRIBUTIONS: usize = 64;

/// Maximum UTF-8 bytes for a display name (§5.5: 128 Unicode scalar values).
pub const MAX_DISPLAY_NAME_UTF16_LEN: usize = 128;

// ---------------------------------------------------------------------------
// Engines (§5.1): three independent compatibility axes.
// ---------------------------------------------------------------------------

/// A SemVer requirement range string (e.g. `>=0.1.0 <0.2.0`), parsed and stored as text.
///
/// Stored as the canonical text so it round-trips and serializes transparently; validation happens
/// at construction via the `semver` crate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct SemverRange(String);

impl SemverRange {
    /// Constructs a validated SemVer requirement range.
    ///
    /// The design writes Cargo-style space-separated comparator sets (e.g. `>=0.1.0 <0.2.0`); the
    /// `semver` crate's `VersionReq` expects comma separators, so whitespace runs are normalized to
    /// commas for validation. The original text is stored verbatim for round-tripping.
    pub fn try_new(value: String) -> Result<Self, ManifestError> {
        Self::validate(&value)?;
        Ok(Self(value))
    }

    fn validate(value: &str) -> Result<(), ManifestError> {
        let normalized = value.split_whitespace().collect::<Vec<_>>().join(",");
        semver::VersionReq::parse(&normalized).map_err(|error| {
            ManifestError::InvalidSemverRange {
                range: value.to_string(),
                reason: error.to_string(),
            }
        })?;
        Ok(())
    }

    /// Returns the range as a `semver` requirement for matching against a concrete version.
    pub fn to_version_req(&self) -> Result<semver::VersionReq, ManifestError> {
        let normalized = self.0.split_whitespace().collect::<Vec<_>>().join(",");
        semver::VersionReq::parse(&normalized).map_err(|error| ManifestError::InvalidSemverRange {
            range: self.0.clone(),
            reason: error.to_string(),
        })
    }

    /// Returns the raw range text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Engine compatibility declarations (§5.1).
///
/// `ora` is the Ora app version range; `pluginApi` is the exact bootstrap↔plugin ABI version
/// (agent v1 must declare `1`); `bun` is the Bun runtime range (agent only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct PluginEngines {
    pub ora: SemverRange,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub plugin_api: Option<u32>,
    #[serde(default, deserialize_with = "strict_option")]
    #[ts(optional)]
    pub bun: Option<SemverRange>,
}

// ---------------------------------------------------------------------------
// Relative entry path (§5.5, §8.2)
// ---------------------------------------------------------------------------

/// A relative path inside the plugin root, validated against escape attempts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(transparent)]
#[ts(export_to = "plugin-protocol.ts", type = "string")]
pub struct PluginRelativePath(String);

impl PluginRelativePath {
    /// Constructs a validated relative path.
    ///
    /// Rejects absolute paths, drive letters, UNC, empty segments, `.`/`..`, trailing dot/space,
    /// ADS (`:`) syntax, and Windows device names. Filesystem containment (regular file under the
    /// plugin root, no reparse points) is checked by the scanner/installer, not here.
    pub fn try_new(value: String) -> Result<Self, ManifestError> {
        if value.is_empty() {
            return Err(ManifestError::InvalidRelativePath {
                reason: "entry path must not be empty",
            });
        }
        if value.as_bytes().contains(&0u8) {
            return Err(ManifestError::InvalidRelativePath {
                reason: "entry path must not contain NUL",
            });
        }
        let normalized = value.replace('\\', "/");
        if normalized.starts_with('/') || normalized.starts_with("//") || normalized.contains(':') {
            return Err(ManifestError::InvalidRelativePath {
                reason: "entry path must be relative (no absolute, UNC, drive or ADS syntax)",
            });
        }
        for segment in normalized.split('/') {
            if segment.is_empty() {
                return Err(ManifestError::InvalidRelativePath {
                    reason: "entry path must not contain empty segments",
                });
            }
            if segment == "." || segment == ".." {
                return Err(ManifestError::InvalidRelativePath {
                    reason: "entry path must not contain '.' or '..' segments",
                });
            }
            if segment.ends_with('.') || segment.ends_with(' ') {
                return Err(ManifestError::InvalidRelativePath {
                    reason: "entry path segment must not end with '.' or space",
                });
            }
            if is_reserved_device_name(segment) {
                return Err(ManifestError::InvalidRelativePath {
                    reason: "entry path segment must not be a reserved Windows device name",
                });
            }
        }
        Ok(Self(value))
    }

    /// Returns the raw relative path.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Returns true when the lowercase base name (before the first dot) is a reserved device name.
fn is_reserved_device_name(segment: &str) -> bool {
    let base = segment.split('.').next().unwrap_or(segment);
    RESERVED_DEVICE_NAMES.contains(&base)
}

const RESERVED_DEVICE_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

// ---------------------------------------------------------------------------
// Contributions (§5.3, §5.4)
// ---------------------------------------------------------------------------

/// The standalone manifest kind tag carried in lifecycle DTOs (§5.3, §12.8).
///
/// [`PluginKindManifest`] carries this tag together with its variant data; lifecycle DTOs such as
/// `$/initialize` need only the tag string, so this enum mirrors the `kind` discriminant without
/// the contribution payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub enum PluginKindTag {
    Agent,
    Workbench,
}

/// One agent contribution declared in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentContribution {
    pub id: AgentProviderId,
    pub display_name: String,
    pub contract_version: u32,
}

/// All agent contributions of one agent-kind manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct AgentContributions {
    pub agents: Vec<AgentContribution>,
}

/// Workbench v1 contribution placeholder (§5.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export_to = "plugin-protocol.ts")]
pub struct WorkbenchContributions {
    pub schema_version: u32,
}

/// Discriminated manifest kind (§5.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export_to = "plugin-protocol.ts")]
pub enum PluginKindManifest {
    Agent {
        main: PluginRelativePath,
        contributes: AgentContributions,
    },
    Workbench {
        contributes: WorkbenchContributions,
    },
}

/// A parsed plugin manifest (the `ora` object of `package.json`).
///
/// Deserialization is performed by [`PluginManifest::parse_from_ora_bytes`] with manual kind routing
/// and strict unknown-field rejection, so this type derives only `Serialize`/`TS` (not
/// `Deserialize`, which would conflict `#[serde(flatten)]` with `deny_unknown_fields`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "plugin-protocol.ts")]
pub struct PluginManifest {
    pub manifest_version: u32,
    pub id: PluginId,
    pub display_name: String,
    #[serde(flatten)]
    #[ts(flatten)]
    pub kind: PluginKindManifest,
    pub engines: PluginEngines,
}

// ---------------------------------------------------------------------------
// Errors and parsing
// ---------------------------------------------------------------------------

/// Errors produced while parsing or validating a plugin manifest.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManifestError {
    #[error("manifest is not a JSON object")]
    NotAnObject,
    #[error("manifest field `{field}` is missing")]
    MissingField { field: &'static str },
    #[error("manifest field `{field}` has an invalid type")]
    InvalidType { field: &'static str },
    #[error("manifest field `{field}` is invalid: {reason}")]
    InvalidValue { field: &'static str, reason: String },
    #[error("manifest contains an unknown field `{field}`")]
    UnknownField { field: String },
    #[error("unsupported manifest version {version}; only {supported} is supported")]
    UnsupportedSchemaVersion { version: u64, supported: u32 },
    #[error("invalid SemVer range `{range}`: {reason}")]
    InvalidSemverRange { range: String, reason: String },
    #[error("invalid relative entry path: {reason}")]
    InvalidRelativePath { reason: &'static str },
    #[error("invalid identity: {0}")]
    Identity(#[from] IdentityError),
}

impl From<crate::json::StrictJsonError> for ManifestError {
    fn from(error: crate::json::StrictJsonError) -> Self {
        ManifestError::InvalidValue {
            field: "ora",
            reason: error.to_string(),
        }
    }
}

/// The recognized top-level keys of the `ora` object, used for unknown-field rejection.
const KNOWN_TOP_LEVEL_KEYS: &[&str] = &[
    "manifestVersion",
    "id",
    "displayName",
    "kind",
    "main",
    "engines",
    "contributes",
];

impl PluginManifest {
    /// Parses the `ora` object from raw `package.json` bytes.
    ///
    /// The bytes must be exactly the `ora` object value (or a full `package.json`; this function
    /// extracts `ora` when present). Strict JSON (duplicate keys, depth) is enforced first, then
    /// strict-schema routing by `manifestVersion` and `kind`.
    pub fn parse_from_ora_bytes(ora_bytes: &[u8]) -> Result<Self, ManifestError> {
        let object = parse_strict_object(ora_bytes, crate::json::DEFAULT_MAX_DEPTH)?;
        Self::parse_from_ora_object(&object)
    }

    /// Parses a pre-decoded `ora` object with strict unknown-field rejection and kind routing.
    pub fn parse_from_ora_object(object: &Map<String, Value>) -> Result<Self, ManifestError> {
        reject_unknown_fields(object, KNOWN_TOP_LEVEL_KEYS)?;

        let manifest_version = take_u64(object, "manifestVersion")?;
        if manifest_version != u64::from(MANIFEST_VERSION_V1) {
            return Err(ManifestError::UnsupportedSchemaVersion {
                version: manifest_version,
                supported: MANIFEST_VERSION_V1,
            });
        }

        let id = PluginId::try_new(take_string(object, "id")?).map_err(ManifestError::from)?;
        let display_name = take_string(object, "displayName")?;
        validate_display_name(&display_name, "displayName")?;
        let engines = serde_json::from_value::<PluginEngines>(take_value(object, "engines")?)
            .map_err(|error| ManifestError::InvalidValue {
                field: "engines",
                reason: error.to_string(),
            })?;

        let kind = take_string(object, "kind")?;
        let manifest_kind = match kind.as_str() {
            "agent" => {
                let main = PluginRelativePath::try_new(take_string(object, "main")?)?;
                let contributes = serde_json::from_value::<AgentContributions>(take_value(
                    object,
                    "contributes",
                )?)
                .map_err(|error| ManifestError::InvalidValue {
                    field: "contributes",
                    reason: error.to_string(),
                })?;
                validate_agent_contributions(&contributes)?;
                PluginKindManifest::Agent { main, contributes }
            }
            "workbench" => {
                let contributes_value = take_value(object, "contributes")?;
                let contributes_map = match contributes_value {
                    Value::Object(map) => map,
                    _ => {
                        return Err(ManifestError::InvalidValue {
                            field: "contributes",
                            reason: "workbench contributes must be an object".to_string(),
                        });
                    }
                };
                for key in contributes_map.keys() {
                    if key != "workbench" {
                        return Err(ManifestError::UnknownField {
                            field: format!("contributes.{key}"),
                        });
                    }
                }
                let workbench_inner = contributes_map.get("workbench").cloned().ok_or(
                    ManifestError::MissingField {
                        field: "contributes.workbench",
                    },
                )?;
                let contributes = serde_json::from_value::<WorkbenchContributions>(workbench_inner)
                    .map_err(|error| ManifestError::InvalidValue {
                        field: "contributes.workbench",
                        reason: error.to_string(),
                    })?;
                if object.contains_key("main") {
                    return Err(ManifestError::InvalidValue {
                        field: "main",
                        reason: "workbench kind must not declare an entry".to_string(),
                    });
                }
                PluginKindManifest::Workbench { contributes }
            }
            other => {
                return Err(ManifestError::InvalidValue {
                    field: "kind",
                    reason: format!("unknown kind `{other}`"),
                });
            }
        };

        Ok(PluginManifest {
            manifest_version: MANIFEST_VERSION_V1,
            id,
            display_name,
            kind: manifest_kind,
            engines,
        })
    }
}

/// Returns `Err(UnknownField)` for any key not in `known`.
fn reject_unknown_fields(object: &Map<String, Value>, known: &[&str]) -> Result<(), ManifestError> {
    for key in object.keys() {
        if !known.contains(&key.as_str()) {
            return Err(ManifestError::UnknownField { field: key.clone() });
        }
    }
    Ok(())
}

/// Extracts a required u64 field.
fn take_u64(object: &Map<String, Value>, field: &'static str) -> Result<u64, ManifestError> {
    match object.get(field) {
        Some(Value::Number(number)) => number.as_u64().ok_or(ManifestError::InvalidType { field }),
        Some(_) => Err(ManifestError::InvalidType { field }),
        None => Err(ManifestError::MissingField { field }),
    }
}

/// Extracts a required string field.
fn take_string(object: &Map<String, Value>, field: &'static str) -> Result<String, ManifestError> {
    match object.get(field) {
        Some(Value::String(value)) => Ok(value.clone()),
        Some(_) => Err(ManifestError::InvalidType { field }),
        None => Err(ManifestError::MissingField { field }),
    }
}

/// Extracts a required value (any JSON, owned) for sub-struct deserialization.
fn take_value(object: &Map<String, Value>, field: &'static str) -> Result<Value, ManifestError> {
    object
        .get(field)
        .cloned()
        .ok_or(ManifestError::MissingField { field })
}

/// Validates a display name against the §5.5 bound (≤128 Unicode scalar values).
fn validate_display_name(value: &str, field: &'static str) -> Result<(), ManifestError> {
    let count = value.chars().count();
    if count > MAX_DISPLAY_NAME_UTF16_LEN {
        return Err(ManifestError::InvalidValue {
            field,
            reason: format!("display name exceeds {MAX_DISPLAY_NAME_UTF16_LEN} scalar values"),
        });
    }
    Ok(())
}

/// Validates agent contributions: count ≤64 and unique local provider ids (§5.2, §5.5).
fn validate_agent_contributions(contributes: &AgentContributions) -> Result<(), ManifestError> {
    if contributes.agents.len() > MAX_CONTRIBUTIONS {
        return Err(ManifestError::InvalidValue {
            field: "contributes.agents",
            reason: format!("exceeds {MAX_CONTRIBUTIONS} contributions"),
        });
    }
    let mut seen = HashSet::new();
    for agent in &contributes.agents {
        if !seen.insert(agent.id.as_str()) {
            return Err(ManifestError::InvalidValue {
                field: "contributes.agents",
                reason: format!("duplicate agent provider id `{}`", agent.id),
            });
        }
        agent.id.validate().map_err(ManifestError::from)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    const AGENT_EXAMPLE: &str = r#"{
        "manifestVersion": 1,
        "id": "ora.claude-code",
        "displayName": "Claude Code",
        "kind": "agent",
        "main": "dist/index.js",
        "engines": {
            "ora": ">=0.1.0 <0.2.0",
            "pluginApi": 1,
            "bun": ">=1.0.0 <2.0.0"
        },
        "contributes": {
            "agents": [
                { "id": "claude-code", "displayName": "Claude Code", "contractVersion": 1 }
            ]
        }
    }"#;

    const WORKBENCH_EXAMPLE: &str = r#"{
        "manifestVersion": 1,
        "id": "ora.example-workbench",
        "displayName": "Example Workbench",
        "kind": "workbench",
        "engines": { "ora": ">=0.1.0 <0.2.0" },
        "contributes": { "workbench": { "schemaVersion": 1 } }
    }"#;

    #[test]
    fn parses_agent_example_from_design() {
        let manifest = PluginManifest::parse_from_ora_bytes(AGENT_EXAMPLE.as_bytes())
            .unwrap_or_else(|error| panic!("parse agent: {error}"));
        assert_eq!(manifest.manifest_version, 1);
        assert_eq!(manifest.id.as_str(), "ora.claude-code");
        assert!(matches!(manifest.kind, PluginKindManifest::Agent { .. }));
    }

    #[test]
    fn parses_workbench_example_from_design() {
        let manifest = PluginManifest::parse_from_ora_bytes(WORKBENCH_EXAMPLE.as_bytes())
            .unwrap_or_else(|error| panic!("parse workbench: {error}"));
        assert!(matches!(
            manifest.kind,
            PluginKindManifest::Workbench { .. }
        ));
    }

    #[test]
    fn rejects_unknown_manifest_version() {
        let bytes = AGENT_EXAMPLE.replace("manifestVersion\": 1", "manifestVersion\": 2");
        let Err(error) = PluginManifest::parse_from_ora_bytes(bytes.as_bytes()) else {
            panic!("unknown version must be rejected");
        };
        assert_eq!(
            error,
            ManifestError::UnsupportedSchemaVersion {
                version: 2,
                supported: 1,
            }
        );
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let bytes = AGENT_EXAMPLE.replace(
            "\"main\": \"dist/index.js\",",
            "\"main\": \"dist/index.js\",\n        \"rogue\": true,",
        );
        assert!(PluginManifest::parse_from_ora_bytes(bytes.as_bytes()).is_err());
    }

    #[test]
    fn rejects_workbench_with_main_and_agent_without_main() {
        let with_main = WORKBENCH_EXAMPLE.replace(
            "\"engines\":",
            "\"main\": \"dist/index.js\",\n        \"engines\":",
        );
        assert!(PluginManifest::parse_from_ora_bytes(with_main.as_bytes()).is_err());

        let without_main = AGENT_EXAMPLE.replace("\"main\": \"dist/index.js\",\n        ", "");
        assert!(PluginManifest::parse_from_ora_bytes(without_main.as_bytes()).is_err());
    }

    #[test]
    fn rejects_duplicate_and_overlong_agent_ids() {
        let dup = AGENT_EXAMPLE.replace(
            "{ \"id\": \"claude-code\", \"displayName\": \"Claude Code\", \"contractVersion\": 1 }",
            "{ \"id\": \"claude-code\", \"displayName\": \"A\", \"contractVersion\": 1 },\n              { \"id\": \"claude-code\", \"displayName\": \"B\", \"contractVersion\": 1 }",
        );
        assert!(PluginManifest::parse_from_ora_bytes(dup.as_bytes()).is_err());
    }

    #[test]
    fn rejects_escape_attempt_in_entry_path() {
        let absolute = AGENT_EXAMPLE.replace("\"dist/index.js\"", "\"/etc/passwd\"");
        assert!(PluginManifest::parse_from_ora_bytes(absolute.as_bytes()).is_err());
        let parent = AGENT_EXAMPLE.replace("\"dist/index.js\"", "\"../evil.js\"");
        assert!(PluginManifest::parse_from_ora_bytes(parent.as_bytes()).is_err());
        let ads = AGENT_EXAMPLE.replace("\"dist/index.js\"", "\"dist:index.js\"");
        assert!(PluginManifest::parse_from_ora_bytes(ads.as_bytes()).is_err());
    }

    #[test]
    fn semver_range_round_trips_and_matches() {
        let range = SemverRange::try_new(">=0.1.0 <0.2.0".to_string())
            .unwrap_or_else(|error| panic!("semver: {error}"));
        let req = range
            .to_version_req()
            .unwrap_or_else(|error| panic!("req: {error}"));
        let inside = semver::Version::parse("0.1.5").unwrap_or_else(|e| panic!("v: {e}"));
        let outside = semver::Version::parse("0.2.0").unwrap_or_else(|e| panic!("v: {e}"));
        assert!(req.matches(&inside));
        assert!(!req.matches(&outside));
        assert!(SemverRange::try_new("not a range".to_string()).is_err());
    }

    #[test]
    fn agent_manifest_serializes_back_to_kind_tagged_json() {
        let manifest = PluginManifest::parse_from_ora_bytes(AGENT_EXAMPLE.as_bytes())
            .unwrap_or_else(|error| panic!("parse: {error}"));
        let value =
            serde_json::to_value(&manifest).unwrap_or_else(|error| panic!("serialize: {error}"));
        assert_eq!(value["kind"], serde_json::json!("agent"));
        assert_eq!(value["main"], serde_json::json!("dist/index.js"));
        assert_eq!(value["engines"]["pluginApi"], serde_json::json!(1));
    }
}
