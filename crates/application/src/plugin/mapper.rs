use ora_contracts::{
    DiscoveredPlugin, Plugin as ContractPlugin, PluginKind as ContractPluginKind,
    PluginProcessEntrypoint, PluginState as ContractPluginState,
};
use ora_domain::{AuditFields, Plugin, PluginId, PluginKind, PluginLifecycleState};

use crate::ApplicationError;

/// Projects a domain plugin into its audit-free public contract form, deserializing the
/// opaque entrypoint JSON back into a typed entrypoint.
pub(crate) fn map_plugin_to_contract(plugin: Plugin) -> Result<ContractPlugin, ApplicationError> {
    let entrypoint: PluginProcessEntrypoint =
        serde_json::from_str(&plugin.entrypoint).map_err(|error| {
            ApplicationError::PluginManifestInvalid {
                message: error.to_string(),
            }
        })?;

    Ok(ContractPlugin {
        id: plugin.id.to_string(),
        version: plugin.version,
        kind: contract_kind_from_domain(plugin.kind),
        entrypoint,
        display_name: plugin.display_name,
        description: plugin.description,
        state: contract_state_from_domain(plugin.state),
        source_path: plugin.source_path,
    })
}

/// Builds a domain plugin from a discovered manifest, serializing the entrypoint for opaque storage.
pub(crate) fn build_plugin_from_discovered(
    discovered: DiscoveredPlugin,
    state: PluginLifecycleState,
    audit_fields: AuditFields,
) -> Result<Plugin, ApplicationError> {
    let manifest = discovered.manifest;
    let entrypoint = serde_json::to_string(&manifest.entrypoint).map_err(|error| {
        ApplicationError::PluginManifestInvalid {
            message: error.to_string(),
        }
    })?;

    Plugin::new(
        PluginId::new(manifest.id),
        domain_kind_from_contract(manifest.kind),
        manifest.version,
        entrypoint,
        manifest.display_name,
        manifest.description,
        state,
        discovered.source_path,
        audit_fields,
    )
    .map_err(ApplicationError::from_plugin_domain_error)
}

fn contract_kind_from_domain(kind: PluginKind) -> ContractPluginKind {
    match kind {
        PluginKind::Agent => ContractPluginKind::Agent,
        PluginKind::Ui => ContractPluginKind::Ui,
        PluginKind::Workbench => ContractPluginKind::Workbench,
    }
}

fn domain_kind_from_contract(kind: ContractPluginKind) -> PluginKind {
    match kind {
        ContractPluginKind::Agent => PluginKind::Agent,
        ContractPluginKind::Ui => PluginKind::Ui,
        ContractPluginKind::Workbench => PluginKind::Workbench,
    }
}

fn contract_state_from_domain(state: PluginLifecycleState) -> ContractPluginState {
    match state {
        PluginLifecycleState::Discovered => ContractPluginState::Discovered,
        PluginLifecycleState::Installed => ContractPluginState::Installed,
        PluginLifecycleState::Enabled => ContractPluginState::Enabled,
        PluginLifecycleState::Started => ContractPluginState::Started,
        PluginLifecycleState::Activated => ContractPluginState::Activated,
    }
}
