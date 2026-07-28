use crate::{AuditFields, DomainModelError, PluginId};
use serde::{Deserialize, Serialize};

/// Categorizes a plugin by which host-side runtime drives it.
///
/// Mirrors `ora_contracts::PluginKind`; kept separate so the domain layer stays
/// independent of the contracts crate. The application mapper converts between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PluginKind {
    /// A plugin that bridges the host to an external agent over ACP.
    Agent,
    /// Reserved for future plugins that contribute UI surfaces.
    #[allow(dead_code)]
    Ui,
    /// Reserved for future plugins that contribute workbench features.
    #[allow(dead_code)]
    Workbench,
}

impl PluginKind {
    /// Returns the integer code used by persistence adapters for this plugin kind.
    pub fn database_value(self) -> i64 {
        match self {
            Self::Agent => 0,
            Self::Ui => 1,
            Self::Workbench => 2,
        }
    }

    /// Converts a persisted integer into a strongly typed plugin kind.
    ///
    /// # Errors
    ///
    /// Returns [`DomainModelError::InvalidPluginKind`] for unknown codes.
    pub fn from_database_value(value: i64) -> Result<Self, DomainModelError> {
        match value {
            0 => Ok(Self::Agent),
            1 => Ok(Self::Ui),
            2 => Ok(Self::Workbench),
            _ => Err(DomainModelError::InvalidPluginKind(value)),
        }
    }
}

/// Lifecycle states a plugin moves through (F1 state machine; see ADR-0001).
///
/// Transitions are enforced by [`PluginLifecycleState::transition_to`]; persistence
/// adapters store the integer code from [`PluginLifecycleState::database_value`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginLifecycleState {
    /// Found by the scanner but not yet registered as installed.
    Discovered,
    /// Registered in storage; not user-enabled.
    Installed,
    /// User-enabled and eligible to be activated on demand; no process running.
    Enabled,
    /// Plugin process spawned and the plugin-channel handshake completed; the kind
    /// runtime is not yet initialized.
    Started,
    /// Kind runtime initialized and ready to execute. For an agent plugin this means
    /// ACP initialize has completed with the agent and a session can be opened.
    Activated,
}

impl PluginLifecycleState {
    /// Returns the integer code used by persistence adapters for this lifecycle state.
    pub fn database_value(self) -> i64 {
        match self {
            Self::Discovered => 0,
            Self::Installed => 1,
            Self::Enabled => 2,
            Self::Started => 3,
            Self::Activated => 4,
        }
    }

    /// Converts a persisted integer into a strongly typed lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns [`DomainModelError::InvalidPluginLifecycleState`] for unknown codes.
    pub fn from_database_value(value: i64) -> Result<Self, DomainModelError> {
        match value {
            0 => Ok(Self::Discovered),
            1 => Ok(Self::Installed),
            2 => Ok(Self::Enabled),
            3 => Ok(Self::Started),
            4 => Ok(Self::Activated),
            _ => Err(DomainModelError::InvalidPluginLifecycleState(value)),
        }
    }

    /// Returns the state reached by a permitted transition, or an error if the move is illegal.
    ///
    /// Permitted F1 transitions:
    /// - `Discovered → Installed` (install)
    /// - `Installed → Enabled` (enable)
    /// - `Enabled → Started` (spawn + handshake)
    /// - `Started → Activated` (kind runtime init)
    /// - `Activated → Started` (deactivate)
    /// - `Started | Activated → Enabled` (stop)
    /// - `Enabled → Installed` (disable)
    ///
    /// Uninstall is modelled as a soft delete, not a state transition.
    ///
    /// # Errors
    ///
    /// Returns [`DomainModelError::InvalidPluginStateTransition`] for disallowed moves.
    pub fn transition_to(self, target: Self) -> Result<Self, DomainModelError> {
        let permitted = matches!(
            (self, target),
            (Self::Discovered, Self::Installed)
                | (Self::Installed, Self::Enabled)
                | (Self::Enabled, Self::Started | Self::Installed)
                | (Self::Started, Self::Activated | Self::Enabled)
                | (Self::Activated, Self::Started | Self::Enabled)
        );

        if permitted {
            Ok(target)
        } else {
            Err(DomainModelError::InvalidPluginStateTransition {
                from_state: self,
                to_state: target,
            })
        }
    }
}

impl TryFrom<i64> for PluginLifecycleState {
    type Error = DomainModelError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::from_database_value(value)
    }
}

/// Persisted plugin record: the installable manifest plus lifecycle state and audit metadata.
///
/// `entrypoint` is the serialized `ora_contracts::PluginProcessEntrypoint` JSON, stored
/// opaquely because the domain layer does not interpret spawn configuration; the runtime
/// deserializes it to spawn the plugin process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plugin {
    pub id: PluginId,
    pub kind: PluginKind,
    pub version: String,
    pub entrypoint: String,
    pub display_name: String,
    pub description: String,
    pub state: PluginLifecycleState,
    pub source_path: String,
    pub audit_fields: AuditFields,
}

impl Plugin {
    /// Creates a plugin record, rejecting a blank manifest version.
    ///
    /// # Errors
    ///
    /// Returns [`DomainModelError::EmptyPluginVersion`] when `version` is blank.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: PluginId,
        kind: PluginKind,
        version: impl Into<String>,
        entrypoint: impl Into<String>,
        display_name: impl Into<String>,
        description: impl Into<String>,
        state: PluginLifecycleState,
        source_path: impl Into<String>,
        audit_fields: AuditFields,
    ) -> Result<Self, DomainModelError> {
        let version = version.into().trim().to_string();
        if version.is_empty() {
            return Err(DomainModelError::EmptyPluginVersion);
        }

        Ok(Self {
            id,
            kind,
            version,
            entrypoint: entrypoint.into(),
            display_name: display_name.into(),
            description: description.into(),
            state,
            source_path: source_path.into(),
            audit_fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Plugin, PluginKind, PluginLifecycleState};
    use crate::{AuditFields, DomainModelError, PluginId};
    use pretty_assertions::assert_eq;

    /// Verifies the integer mapping round-trips every lifecycle state.
    #[test]
    fn lifecycle_state_round_trips_through_database_value() {
        for state in [
            PluginLifecycleState::Discovered,
            PluginLifecycleState::Installed,
            PluginLifecycleState::Enabled,
            PluginLifecycleState::Started,
            PluginLifecycleState::Activated,
        ] {
            assert_eq!(
                PluginLifecycleState::from_database_value(state.database_value()).unwrap(),
                state
            );
        }
    }

    /// Verifies permitted forward and backward transitions succeed.
    #[test]
    fn permits_valid_lifecycle_transitions() {
        use PluginLifecycleState as S;
        assert_eq!(
            S::Discovered.transition_to(S::Installed).unwrap(),
            S::Installed
        );
        assert_eq!(S::Installed.transition_to(S::Enabled).unwrap(), S::Enabled);
        assert_eq!(S::Enabled.transition_to(S::Started).unwrap(), S::Started);
        assert_eq!(
            S::Started.transition_to(S::Activated).unwrap(),
            S::Activated
        );
        assert_eq!(S::Activated.transition_to(S::Started).unwrap(), S::Started);
        assert_eq!(S::Started.transition_to(S::Enabled).unwrap(), S::Enabled);
        assert_eq!(S::Activated.transition_to(S::Enabled).unwrap(), S::Enabled);
        assert_eq!(
            S::Enabled.transition_to(S::Installed).unwrap(),
            S::Installed
        );
    }

    /// Verifies illegal transitions are rejected, including skipping states.
    #[test]
    fn rejects_invalid_lifecycle_transitions() {
        use PluginLifecycleState as S;
        assert!(matches!(
            S::Discovered.transition_to(S::Enabled),
            Err(DomainModelError::InvalidPluginStateTransition { .. })
        ));
        assert!(matches!(
            S::Discovered.transition_to(S::Activated),
            Err(DomainModelError::InvalidPluginStateTransition { .. })
        ));
        assert!(matches!(
            S::Installed.transition_to(S::Started),
            Err(DomainModelError::InvalidPluginStateTransition { .. })
        ));
        assert!(matches!(
            S::Activated.transition_to(S::Discovered),
            Err(DomainModelError::InvalidPluginStateTransition { .. })
        ));
    }

    /// Verifies an unknown integer code is rejected as an invalid lifecycle state.
    #[test]
    fn rejects_unknown_lifecycle_database_value() {
        assert!(matches!(
            PluginLifecycleState::from_database_value(99),
            Err(DomainModelError::InvalidPluginLifecycleState(99))
        ));
    }

    /// Verifies a blank manifest version is rejected at construction time.
    #[test]
    fn rejects_blank_plugin_version() {
        let result = Plugin::new(
            PluginId::new("codex"),
            PluginKind::Agent,
            "   ",
            r#"{"program":"node"}"#,
            "Codex",
            "Bridges Ora to the codex agent over ACP",
            PluginLifecycleState::Installed,
            "plugins/codex",
            AuditFields::new(0, 0, false),
        );
        assert!(matches!(result, Err(DomainModelError::EmptyPluginVersion)));
    }
}
