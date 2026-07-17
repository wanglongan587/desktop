//! Plugin resource/size caps, centralized in one typed config (design-v3 §5.5, §12.7).
//!
//! The design forbids scattered magic numbers: file/dir/byte/manifest/frame/JSON/pending/queue
//! budgets all live here, injected by production composition or test fixtures. Host policy may
//! tighten these but never raise them above the v1 hard caps; `max_frame_bytes` must equal the wire
//! v1 constant (§12.8). Opaque id/config-key caps are fixed wire constants (§13.1), not here.

use ora_plugin_protocol::{InitializeLimits, MAX_PAYLOAD_BYTES};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// All plugin resource/size caps (§5.5, §12.7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginLimits {
    // --- manifest/package caps (§5.5) ---
    /// Maximum `package.json` size in bytes (§5.5: 256 KiB).
    pub max_package_json_bytes: u32,
    /// Maximum display-name Unicode scalar count (§5.5: 128).
    pub max_manifest_display_name_scalars: u32,
    /// Maximum entry relative path UTF-8 bytes (§5.5: 512).
    pub max_entry_path_bytes: u32,
    /// Maximum contribution count per manifest (§5.5: 64).
    pub max_contributions: u32,
    /// Maximum JSON nesting depth (§5.5: 64).
    pub max_json_depth: u32,
    // --- file/dir caps (§5.5, §6.3) ---
    /// Maximum regular-file count (§6.3: 10,000).
    pub max_file_count: u32,
    /// Maximum single-file size (§6.3: 64 MiB).
    pub max_single_file_bytes: u32,
    /// Maximum total tree size (§6.3: 512 MiB).
    pub max_total_bytes: u32,
    /// Maximum directory depth (§5.5: 64).
    pub max_directory_depth: u32,
    // --- wire/runtime caps (§12.7) ---
    /// Maximum frame payload bytes; must equal the wire v1 constant (§12.8: 8 MiB).
    pub max_frame_bytes: u32,
    /// Maximum ordinary pending requests per connection (§12.7: 128).
    pub max_pending_requests: u32,
    /// Maximum single stream-event payload (§12.7: 256 KiB).
    pub max_agent_event_bytes: u32,
    /// Maximum terminal result/error payload (§12.7: 1 MiB).
    pub max_agent_result_bytes: u32,
    /// Maximum prompt size (§12.7: 1 MiB).
    pub max_agent_prompt_bytes: u32,
    /// Maximum active provisional+bound turns per plugin (§12.7: 64).
    pub max_active_turns: u32,
    /// Maximum page items (§12.7: 100).
    pub max_page_items: u32,
    // --- writer queue caps (§12.7) ---
    /// Maximum queued frames per writer (§12.7: 256).
    pub max_writer_queue_frames: u32,
    /// Maximum queued bytes per writer (§12.7: 16 MiB).
    pub max_writer_queue_bytes: u32,
    /// Non-borrowable control-reserve frames (§12.7: ≥32 of the 256).
    pub control_reserve_frames: u32,
    /// Non-borrowable control-reserve bytes (§12.7: ≥2 MiB of the 16 MiB).
    pub control_reserve_bytes: u32,
}

/// Errors produced when validating [`PluginLimits`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PluginLimitsError {
    #[error("max_frame_bytes ({actual}) must equal the wire v1 constant ({expected})")]
    MaxFrameBytesMismatch { actual: u32, expected: u32 },
    #[error("{field} must be > 0")]
    NonPositive { field: &'static str },
    #[error("control reserve ({reserve}) must be <= total ({total}) for {dimension}")]
    ReserveExceedsTotal {
        dimension: &'static str,
        reserve: u32,
        total: u32,
    },
}

const KIB: u32 = 1024;
const MIB: u32 = 1024 * 1024;

impl PluginLimits {
    /// The MVP-default caps (§5.5, §6.3, §12.7).
    #[allow(clippy::too_many_arguments)]
    pub fn mvp() -> Self {
        Self {
            max_package_json_bytes: 256 * KIB,
            max_manifest_display_name_scalars: 128,
            max_entry_path_bytes: 512,
            max_contributions: 64,
            max_json_depth: 64,
            max_file_count: 10_000,
            max_single_file_bytes: 64 * MIB,
            max_total_bytes: 512 * MIB,
            max_directory_depth: 64,
            max_frame_bytes: MAX_PAYLOAD_BYTES.try_into().unwrap_or(8 * MIB),
            max_pending_requests: 128,
            max_agent_event_bytes: 256 * KIB,
            max_agent_result_bytes: MIB,
            max_agent_prompt_bytes: MIB,
            max_active_turns: 64,
            max_page_items: 100,
            max_writer_queue_frames: 256,
            max_writer_queue_bytes: 16 * MIB,
            control_reserve_frames: 32,
            control_reserve_bytes: 2 * MIB,
        }
    }

    /// Validates the caps against the §5.5/§12.7 invariants.
    ///
    /// `max_frame_bytes` must equal the wire v1 constant; all caps are positive; control reserves
    /// fit within the writer-queue totals. Detailed per-reserve-vs-max-frame invariants are the
    /// runtime's concern (§12.7); this catches the configuration-level errors.
    pub fn validate(&self) -> Result<(), PluginLimitsError> {
        if self.max_frame_bytes != u32::try_from(MAX_PAYLOAD_BYTES).unwrap_or(0) {
            return Err(PluginLimitsError::MaxFrameBytesMismatch {
                actual: self.max_frame_bytes,
                expected: u32::try_from(MAX_PAYLOAD_BYTES).unwrap_or(0),
            });
        }
        for (field, value) in [
            ("max_package_json_bytes", self.max_package_json_bytes),
            (
                "max_manifest_display_name_scalars",
                self.max_manifest_display_name_scalars,
            ),
            ("max_entry_path_bytes", self.max_entry_path_bytes),
            ("max_contributions", self.max_contributions),
            ("max_json_depth", self.max_json_depth),
            ("max_file_count", self.max_file_count),
            ("max_single_file_bytes", self.max_single_file_bytes),
            ("max_total_bytes", self.max_total_bytes),
            ("max_directory_depth", self.max_directory_depth),
            ("max_frame_bytes", self.max_frame_bytes),
            ("max_pending_requests", self.max_pending_requests),
            ("max_agent_event_bytes", self.max_agent_event_bytes),
            ("max_agent_result_bytes", self.max_agent_result_bytes),
            ("max_agent_prompt_bytes", self.max_agent_prompt_bytes),
            ("max_active_turns", self.max_active_turns),
            ("max_page_items", self.max_page_items),
            ("max_writer_queue_frames", self.max_writer_queue_frames),
            ("max_writer_queue_bytes", self.max_writer_queue_bytes),
            ("control_reserve_frames", self.control_reserve_frames),
            ("control_reserve_bytes", self.control_reserve_bytes),
        ] {
            if value == 0 {
                return Err(PluginLimitsError::NonPositive { field });
            }
        }
        if self.control_reserve_frames > self.max_writer_queue_frames {
            return Err(PluginLimitsError::ReserveExceedsTotal {
                dimension: "frames",
                reserve: self.control_reserve_frames,
                total: self.max_writer_queue_frames,
            });
        }
        if self.control_reserve_bytes > self.max_writer_queue_bytes {
            return Err(PluginLimitsError::ReserveExceedsTotal {
                dimension: "bytes",
                reserve: self.control_reserve_bytes,
                total: self.max_writer_queue_bytes,
            });
        }
        Ok(())
    }

    /// Derives the seven `$/initialize` `limits` (§12.8) from this config.
    pub fn to_initialize_limits(&self) -> InitializeLimits {
        InitializeLimits {
            max_frame_bytes: self.max_frame_bytes,
            max_pending_requests: self.max_pending_requests,
            max_agent_event_bytes: self.max_agent_event_bytes,
            max_agent_result_bytes: self.max_agent_result_bytes,
            max_agent_prompt_bytes: self.max_agent_prompt_bytes,
            max_active_turns: self.max_active_turns,
            max_page_items: self.max_page_items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn mvp_defaults_match_design_and_validate() {
        let limits = PluginLimits::mvp();
        assert_eq!(limits.max_package_json_bytes, 256 * KIB);
        assert_eq!(limits.max_file_count, 10_000);
        assert_eq!(limits.max_total_bytes, 512 * MIB);
        assert_eq!(limits.max_frame_bytes, 8 * MIB);
        assert_eq!(limits.max_page_items, 100);
        assert_eq!(limits.control_reserve_frames, 32);
        assert!(limits.validate().is_ok());
    }

    #[test]
    fn max_frame_bytes_must_equal_wire_constant() {
        let mut limits = PluginLimits::mvp();
        limits.max_frame_bytes = 4 * MIB;
        assert_eq!(
            limits.validate(),
            Err(PluginLimitsError::MaxFrameBytesMismatch {
                actual: 4 * MIB,
                expected: 8 * MIB
            })
        );
    }

    #[test]
    fn control_reserve_must_fit_within_writer_queue() {
        let mut limits = PluginLimits::mvp();
        limits.control_reserve_frames = limits.max_writer_queue_frames + 1;
        assert!(matches!(
            limits.validate(),
            Err(PluginLimitsError::ReserveExceedsTotal {
                dimension: "frames",
                ..
            })
        ));
    }

    #[test]
    fn to_initialize_limits_maps_the_seven() {
        let limits = PluginLimits::mvp();
        let init = limits.to_initialize_limits();
        assert_eq!(init.max_frame_bytes, limits.max_frame_bytes);
        assert_eq!(init.max_pending_requests, limits.max_pending_requests);
        assert_eq!(init.max_agent_event_bytes, limits.max_agent_event_bytes);
        assert_eq!(init.max_agent_result_bytes, limits.max_agent_result_bytes);
        assert_eq!(init.max_agent_prompt_bytes, limits.max_agent_prompt_bytes);
        assert_eq!(init.max_active_turns, limits.max_active_turns);
        assert_eq!(init.max_page_items, limits.max_page_items);
    }
}
