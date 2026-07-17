//! User enablement intent and derived effective enablement (design-v3 §7.2).
//!
//! Only [`UserEnablement`] is persisted; [`EffectiveEnablement`] is a derived value computed from
//! the catalog, receipt/integrity, engines, runtime support, pending operations and crash policy.
//! Multiple disable reasons may be active at once — the registry/admission uses a single *primary*
//! reason chosen by the strict total order in §7.2:
//!
//! `PendingRemoval > MissingInstallFiles > IntegrityMismatch > InvalidManifest >
//! IncompatibleEngine > UnsupportedKind > Policy > CrashLoop > User` (and `Enabled` below all).
//!
//! High-priority reasons generally do not overwrite user intent; the one exception (§7.2) is that
//! MVP does not allow a new `Enabled` intent for an unsupported `workbench`, so a future executor
//! cannot silently gain runtime admission.

use serde::{Deserialize, Serialize};

/// The user's persisted enablement intent (§7.2). Only this is stored in state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserEnablement {
    Enabled,
    Disabled,
}

/// The derived effective state (§7.2): `Enabled`, or `Disabled` with the primary reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectiveEnablement {
    Enabled,
    Disabled(EffectiveDisableReason),
}

/// Why an installed plugin is effectively disabled (§7.2).
///
/// Variants are ordered by the strict total order in [`EffectiveDisableReason::priority`]; this is
/// *not* the declaration order, so [`Ord`] is intentionally not derived.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectiveDisableReason {
    User,
    InvalidManifest,
    IncompatibleEngine,
    UnsupportedKind,
    IntegrityMismatch,
    PendingRemoval,
    MissingInstallFiles,
    Policy,
    CrashLoop,
}

impl EffectiveDisableReason {
    /// Returns the strict total-order priority from §7.2 (higher = more blocking).
    ///
    /// `PendingRemoval` is highest; `User` is lowest among disable reasons.
    pub const fn priority(self) -> u8 {
        match self {
            EffectiveDisableReason::PendingRemoval => 9,
            EffectiveDisableReason::MissingInstallFiles => 8,
            EffectiveDisableReason::IntegrityMismatch => 7,
            EffectiveDisableReason::InvalidManifest => 6,
            EffectiveDisableReason::IncompatibleEngine => 5,
            EffectiveDisableReason::UnsupportedKind => 4,
            EffectiveDisableReason::Policy => 3,
            EffectiveDisableReason::CrashLoop => 2,
            EffectiveDisableReason::User => 1,
        }
    }
}

/// Selects the single primary disable reason from a set of active reasons (§7.2).
///
/// Returns the reason with the highest [`priority`](EffectiveDisableReason::priority); an empty set
/// means no disable reason is active (the plugin is `Enabled`).
pub fn primary_reason(reasons: &[EffectiveDisableReason]) -> Option<EffectiveDisableReason> {
    reasons
        .iter()
        .copied()
        .max_by_key(|reason| reason.priority())
}

impl EffectiveEnablement {
    /// Computes the effective enablement from a user intent and the active disable reasons.
    ///
    /// If any disable reason is active, it wins over an `Enabled` intent and the primary reason is
    /// reported; otherwise the user intent is effective.
    pub fn from(user: UserEnablement, reasons: &[EffectiveDisableReason]) -> Self {
        match primary_reason(reasons) {
            Some(reason) => EffectiveEnablement::Disabled(reason),
            None => match user {
                UserEnablement::Enabled => EffectiveEnablement::Enabled,
                UserEnablement::Disabled => {
                    EffectiveEnablement::Disabled(EffectiveDisableReason::User)
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// The strict total order from §7.2, pairwise.
    #[test]
    fn disable_reason_total_order_matches_design() {
        // Ordered high → low.
        let ordered = [
            EffectiveDisableReason::PendingRemoval,
            EffectiveDisableReason::MissingInstallFiles,
            EffectiveDisableReason::IntegrityMismatch,
            EffectiveDisableReason::InvalidManifest,
            EffectiveDisableReason::IncompatibleEngine,
            EffectiveDisableReason::UnsupportedKind,
            EffectiveDisableReason::Policy,
            EffectiveDisableReason::CrashLoop,
            EffectiveDisableReason::User,
        ];
        for window in ordered.windows(2) {
            let [higher, lower] = [window[0], window[1]];
            assert!(
                higher.priority() > lower.priority(),
                "{higher:?} (p={}) must outrank {lower:?} (p={})",
                higher.priority(),
                lower.priority()
            );
        }
    }

    #[test]
    fn primary_reason_picks_highest_priority() {
        let reasons = [
            EffectiveDisableReason::User,
            EffectiveDisableReason::IntegrityMismatch,
            EffectiveDisableReason::InvalidManifest,
        ];
        assert_eq!(
            primary_reason(&reasons),
            Some(EffectiveDisableReason::IntegrityMismatch)
        );
        assert_eq!(primary_reason(&[]), None);
        // PendingRemoval beats everything.
        assert_eq!(
            primary_reason(&[
                EffectiveDisableReason::CrashLoop,
                EffectiveDisableReason::PendingRemoval,
                EffectiveDisableReason::UnsupportedKind,
            ]),
            Some(EffectiveDisableReason::PendingRemoval)
        );
    }

    #[test]
    fn effective_enablement_from_user_and_reasons() {
        // Active reason wins over Enabled intent.
        assert_eq!(
            EffectiveEnablement::from(
                UserEnablement::Enabled,
                &[EffectiveDisableReason::UnsupportedKind]
            ),
            EffectiveEnablement::Disabled(EffectiveDisableReason::UnsupportedKind)
        );
        // No reason + Enabled → Enabled.
        assert_eq!(
            EffectiveEnablement::from(UserEnablement::Enabled, &[]),
            EffectiveEnablement::Enabled
        );
        // No reason + Disabled → Disabled(User).
        assert_eq!(
            EffectiveEnablement::from(UserEnablement::Disabled, &[]),
            EffectiveEnablement::Disabled(EffectiveDisableReason::User)
        );
        // Multiple reasons → primary.
        assert_eq!(
            EffectiveEnablement::from(
                UserEnablement::Enabled,
                &[
                    EffectiveDisableReason::User,
                    EffectiveDisableReason::PendingRemoval
                ]
            ),
            EffectiveEnablement::Disabled(EffectiveDisableReason::PendingRemoval)
        );
    }

    #[test]
    fn user_enablement_serializes_camelcase() {
        assert_eq!(
            serde_json::to_value(UserEnablement::Enabled)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            serde_json::json!("enabled")
        );
        assert_eq!(
            serde_json::to_value(UserEnablement::Disabled)
                .unwrap_or_else(|e| panic!("serialize: {e}")),
            serde_json::json!("disabled")
        );
    }
}
