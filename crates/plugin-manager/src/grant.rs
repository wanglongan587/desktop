//! Plugin launch-grant model (design-v3 §14.3).
//!
//! A `PluginLaunchGrant` binds `plugin_id + content_owner + grant_schema_version + revision` and
//! records only *references* to Host configuration/credentials/discovered executables/authorized
//! paths — never secret values (§14.3: state persists references + authorization metadata only;
//! values are resolved into protected memory at launch time by `LaunchValueResolver`). Grants are
//! separate from plugin content; uninstall removes them, reinstall never silently inherits them.

use std::ffi::OsString;
use std::future::Future;

use ora_plugin_protocol::{ContentOwnerId, PluginId};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::registry::AgentProviderKey;

/// A monotonic grant revision (§14.3). Bumped on every `set_launch_grant`/revoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GrantRevision(pub u64);

impl GrantRevision {
    pub fn initial() -> Self {
        Self(1)
    }
    pub fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
    pub fn get(self) -> u64 {
        self.0
    }
}

/// A Windows environment-variable name (§14.3). Non-empty, no `=`/NUL.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EnvironmentVariableName(String);

impl EnvironmentVariableName {
    pub fn try_new(value: String) -> Result<Self, LaunchGrantError> {
        if value.is_empty() || value.contains('=') || value.as_bytes().contains(&0u8) {
            return Err(LaunchGrantError::InvalidEnvironmentVariableName);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque Host-configuration key referenced by a grant (§14.3). Persisted as a reference only.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ConfigurationKey(String);

impl ConfigurationKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque credential key referenced by a grant (§14.3). The resolved value is a secret.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CredentialKey(String);

impl CredentialKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An opaque authorized-path id referenced by a grant (§14.3).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthorizedPathId(String);

impl AuthorizedPathId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A resolved secret value held in protected memory at launch (§14.3). Never serialized or logged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
    /// Reveals the secret for process launch only; must not be logged.
    pub fn reveal(&self) -> &str {
        &self.0
    }
}

/// A reference to a Host-injected launch value (§14.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum LaunchValueReference {
    HostConfiguration { key: ConfigurationKey },
    Credential { key: CredentialKey },
    DiscoveredExecutable { provider: AgentProviderKey },
    AuthorizedPath { path_id: AuthorizedPathId },
}

/// One environment binding: target variable → reference (§14.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnvironmentBinding {
    pub target: EnvironmentVariableName,
    pub value: LaunchValueReference,
}

/// A user-approved launch grant (§14.3). Persisted as references + metadata, never secret values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginLaunchGrant {
    pub plugin_id: PluginId,
    pub content_owner: ContentOwnerId,
    pub schema_version: u32,
    pub revision: GrantRevision,
    pub environment: Vec<EnvironmentBinding>,
}

/// A resolved launch value (in-memory only, §14.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedLaunchValue {
    Plain { value: OsString },
    Secret { value: SecretValue },
}

/// Why a launch value could not be resolved (§14.3).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LaunchValueResolutionError {
    #[error("configuration key is missing")]
    ConfigurationMissing,
    #[error("credential is missing")]
    CredentialMissing,
    #[error("credential is locked")]
    CredentialLocked,
    #[error("discovered executable is missing")]
    DiscoveredExecutableMissing,
    #[error("path is not authorized")]
    PathNotAuthorized,
}

/// Errors produced while constructing launch-grant values.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LaunchGrantError {
    #[error("invalid environment variable name")]
    InvalidEnvironmentVariableName,
}

/// Resolves user-authorized launch references only at process launch time (§14.3).
///
/// Implementations must not persist or log resolved values, including paths.
pub trait LaunchValueResolver {
    /// Resolves one stored reference or reports that the grant is unavailable.
    fn resolve(
        &self,
        reference: &LaunchValueReference,
    ) -> impl Future<Output = Result<ResolvedLaunchValue, LaunchValueResolutionError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn launch_value_reference_projects_typed_union() {
        let r = LaunchValueReference::Credential {
            key: CredentialKey::new("ANTHROPIC_API_KEY"),
        };
        assert_eq!(
            serde_json::to_value(&r).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "type": "credential", "key": "ANTHROPIC_API_KEY" })
        );
        let p = LaunchValueReference::AuthorizedPath {
            path_id: AuthorizedPathId::new("bin"),
        };
        assert_eq!(
            serde_json::to_value(&p).unwrap_or_else(|e| panic!("serialize: {e}")),
            json!({ "type": "authorizedPath", "pathId": "bin" })
        );
    }

    #[test]
    fn environment_variable_name_validates() {
        assert!(EnvironmentVariableName::try_new("PATH".to_string()).is_ok());
        assert!(EnvironmentVariableName::try_new("A=B".to_string()).is_err());
        assert!(EnvironmentVariableName::try_new("".to_string()).is_err());
    }

    #[test]
    fn grant_revision_is_monotonic() {
        let r0 = GrantRevision::initial();
        assert_eq!(r0.get(), 1);
        assert_eq!(r0.next().get(), 2);
    }
}
