/**
 * Width budget for the right-hand Spec panel.
 *
 * Tuned to Codex Desktop's secondary/document side panel: a generous default for
 * reading markdown, a floor that still fits list + reader, and a high ceiling so
 * the panel can grow past half the window the way Codex's browser/diff pane does.
 * Runtime clamping always leaves a usable strip for the chat column.
 */
export const SPEC_PANEL_MIN_WIDTH = 400;
export const SPEC_PANEL_DEFAULT_WIDTH = 640;
export const SPEC_PANEL_MAX_WIDTH = 1400;

/** Below this panel width the list collapses into a compact picker. */
export const SPEC_PANEL_COMPACT_BREAKPOINT = 520;

/** Open/close slide duration; matches the shell's other cubic-bezier discloses. */
export const SPEC_PANEL_ANIMATION_MS = 280;

/**
 * Minimum pixels kept for everything left of the Spec frame (workspace sidebar +
 * chat). Codex lets the side panel dominate; we only protect a narrow chat strip.
 */
export const SPEC_PANEL_MIN_REMAINING_SHELL_WIDTH = 360;

/** Clamps a candidate panel width into the configured Spec panel range. */
export function clampSpecPanelWidth(width: number): number {
  return Math.min(SPEC_PANEL_MAX_WIDTH, Math.max(SPEC_PANEL_MIN_WIDTH, Math.round(width)));
}

/**
 * Caps the panel so the workspace keeps a usable share of the viewport while the
 * user drags the Spec edge. Prefer this over {@link clampSpecPanelWidth} during
 * pointer moves so a wide monitor can actually use the high max.
 */
export function clampSpecPanelWidthForShell(
  width: number,
  shellWidth: number,
  reservedWidth: number = SPEC_PANEL_MIN_REMAINING_SHELL_WIDTH,
): number {
  const room = Math.max(SPEC_PANEL_MIN_WIDTH, shellWidth - reservedWidth);
  return Math.min(clampSpecPanelWidth(width), room);
}
