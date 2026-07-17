mod catalog;
mod config;
mod digest;
mod enablement;
mod error;
mod facade;
mod failure;
mod grant;
mod layout;
mod limits;
mod manager;
mod plugin_error;
mod receipt;
mod registry;
mod results;
mod state;

pub use catalog::{
    CatalogEntry, CompatibilityReason, IntegrityStatus, ManifestValidity, PluginCatalogSnapshot,
    PluginDiagnostic, RuntimeCompatibility, RuntimeSupport,
};
pub use config::PluginManagerConfig;
pub use digest::{FileEntry, TreeDigest, compute_tree_digest};
pub use enablement::{EffectiveDisableReason, EffectiveEnablement, UserEnablement, primary_reason};
pub use error::PluginManagerError;
pub use facade::{
    CandidateHandle, DataRemovalScope, DiscoveryRootId, GrantBindingKey, SelectionHandle,
    StopReason,
};
pub use failure::{
    AgentContractFailure, FatalSettlementCause, TransportFailureStage, UnknownOutcomeCause,
};
pub use grant::{
    AuthorizedPathId, ConfigurationKey, CredentialKey, EnvironmentBinding, EnvironmentVariableName,
    GrantRevision, LaunchGrantError, LaunchValueReference, LaunchValueResolutionError,
    LaunchValueResolver, PluginLaunchGrant, ResolvedLaunchValue, SecretValue,
};
pub use layout::PluginLayout;
pub use limits::{PluginLimits, PluginLimitsError};
pub use manager::{PluginLifecycleState, PluginManager};
pub use plugin_error::{
    ActivationFailure, CandidateHandleFailure, DeactivationFailure, HandshakeFailure, PluginError,
    PluginKind, ProtocolFailure, ReviewedPluginIdentity, SelectionHandleFailure,
    SourceChangeReason, SourceRootIdentity,
};
pub use receipt::{RECEIPT_VERSION_V1, Receipt, ReceiptSource};
pub use registry::{
    AgentProviderKey, RegisteredAgent, RegisteredPlugin, RegistryDelta, RegistrySnapshot,
};
pub use results::{CandidateSelection, IdentifiedPlugin, InstalledPlugin};
pub use state::{
    CandidateAuditId, ContentDigest, CrashPolicy, Installation, InstallationState,
    ManagedTrashLocation, OperationId, PendingInstall, PendingInstallPhase, PendingOperation,
    PendingRemoval, PendingRemovalPhase, PluginStateEntry, StateModelError,
};
