//! Runtime registry: immutable snapshot + revisioned delta (design-v3 §7.3).
//!
//! `RuntimeRegistry` holds only `EffectiveEnablement::Enabled`, runtime-supported
//! `AgentContribution`s. It is an immutable snapshot plus a monotonic `revision`; the single
//! writer computes a delta on catalog/effective-enablement changes, commits the snapshot, then
//! notifies consumers (§7.3). `installed ≠ registered ≠ running`: the registry is a separate
//! state from the catalog and the live runtime.

use std::collections::{HashMap, HashSet};

use ora_plugin_protocol::{AgentProviderId, JsonSafeU64, PluginId};
use serde::{Deserialize, Serialize};

/// Global agent provider key: `(PluginId, AgentProviderId)` (§5.2, §7.3).
///
/// Built as a struct (not a bare concatenated string) so identity is not constructed by unescaped
/// string joins. Wire only carries the local `AgentProviderId`; this key is application-internal.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentProviderKey {
    pub plugin_id: PluginId,
    pub provider_id: AgentProviderId,
}

/// One registered agent contribution (§7.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegisteredAgent {
    pub provider_id: AgentProviderId,
    pub contract_version: u32,
}

/// One registered plugin (§7.3): a plugin with at least one enabled, supported agent contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegisteredPlugin {
    pub version: String,
}

/// An immutable registry snapshot (§7.3).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegistrySnapshot {
    pub revision: JsonSafeU64,
    pub agents_by_provider: HashMap<AgentProviderKey, RegisteredAgent>,
    pub plugins_by_id: HashMap<PluginId, RegisteredPlugin>,
}

/// A revisioned delta between two snapshots (§7.3, §16.2 `RegistryChanged`).
///
/// `added`/`removed` are canonical plugin ids whose effective-enabled agent contributions changed
/// membership. Consumers that only need "revision changed" can re-snapshot instead of applying the
/// delta.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegistryDelta {
    pub revision: JsonSafeU64,
    pub added: Vec<PluginId>,
    pub removed: Vec<PluginId>,
}

impl RegistrySnapshot {
    /// An empty snapshot at revision 0.
    pub fn empty() -> Self {
        Self {
            revision: JsonSafeU64::try_new(0)
                .unwrap_or_else(|error| panic!("zero registry revision: {error}")),
            agents_by_provider: HashMap::new(),
            plugins_by_id: HashMap::new(),
        }
    }

    /// Computes the delta from `self` (older) to `next` (newer): plugin ids present in `next` but
    /// not `self` are added; present in `self` but not `next` are removed (§7.3).
    ///
    /// The returned `revision` is `next.revision`; a single-writer must assign strictly monotonic
    /// revisions so a long-running scan cannot overwrite a newer disable with an older snapshot.
    pub fn diff(&self, next: &RegistrySnapshot) -> RegistryDelta {
        let prev: HashSet<&PluginId> = self.plugins_by_id.keys().collect();
        let next_keys: HashSet<&PluginId> = next.plugins_by_id.keys().collect();
        let mut added: Vec<PluginId> = next_keys.difference(&prev).copied().cloned().collect();
        let mut removed: Vec<PluginId> = prev.difference(&next_keys).copied().cloned().collect();
        added.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        removed.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        RegistryDelta {
            revision: next.revision,
            added,
            removed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn pid(s: &str) -> PluginId {
        PluginId::try_new(s.to_string()).unwrap_or_else(|e| panic!("pid: {e}"))
    }

    fn provider(s: &str) -> AgentProviderId {
        AgentProviderId::try_new(s.to_string()).unwrap_or_else(|e| panic!("provider: {e}"))
    }

    fn rev(n: u64) -> JsonSafeU64 {
        JsonSafeU64::try_new(n).unwrap_or_else(|e| panic!("revision {n}: {e}"))
    }

    #[test]
    fn empty_snapshot_diffs_to_added_ids() {
        let mut next = RegistrySnapshot::empty();
        next.revision = rev(1);
        next.plugins_by_id.insert(
            pid("ora.a"),
            RegisteredPlugin {
                version: "0.1.0".to_string(),
            },
        );
        let delta = RegistrySnapshot::empty().diff(&next);
        assert_eq!(delta.revision.get(), 1);
        assert_eq!(delta.added, vec![pid("ora.a")]);
        assert!(delta.removed.is_empty());
    }

    #[test]
    fn diff_reports_added_and_removed_sorted() {
        let mut prev = RegistrySnapshot::empty();
        prev.revision = rev(2);
        prev.plugins_by_id.insert(
            pid("ora.a"),
            RegisteredPlugin {
                version: "0.1.0".to_string(),
            },
        );
        prev.plugins_by_id.insert(
            pid("ora.b"),
            RegisteredPlugin {
                version: "0.1.0".to_string(),
            },
        );

        let mut next = RegistrySnapshot::empty();
        next.revision = rev(3);
        next.plugins_by_id.insert(
            pid("ora.b"),
            RegisteredPlugin {
                version: "0.1.0".to_string(),
            },
        );
        next.plugins_by_id.insert(
            pid("ora.c"),
            RegisteredPlugin {
                version: "0.1.0".to_string(),
            },
        );

        let delta = prev.diff(&next);
        assert_eq!(delta.revision.get(), 3);
        assert_eq!(delta.added, vec![pid("ora.c")]);
        assert_eq!(delta.removed, vec![pid("ora.a")]);
    }

    #[test]
    fn agent_provider_key_distinguishes_plugin_and_provider() {
        let k1 = AgentProviderKey {
            plugin_id: pid("ora.claude-code"),
            provider_id: provider("claude-code"),
        };
        let k2 = AgentProviderKey {
            plugin_id: pid("ora.codex"),
            provider_id: provider("claude-code"),
        };
        assert_ne!(k1, k2);
        let mut map = HashMap::new();
        map.insert(
            k1.clone(),
            RegisteredAgent {
                provider_id: provider("claude-code"),
                contract_version: 1,
            },
        );
        assert!(map.contains_key(&k1));
        assert!(!map.contains_key(&k2));
    }
}
