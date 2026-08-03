import { describe, expect, it } from "vitest";
import {
  SPEC_PANEL_MAX_WIDTH,
  SPEC_PANEL_MIN_REMAINING_SHELL_WIDTH,
  SPEC_PANEL_MIN_WIDTH,
  clampSpecPanelWidth,
  clampSpecPanelWidthForShell,
} from "./spec-panel-layout";

describe("clampSpecPanelWidth", () => {
  it("keeps values inside the Codex-like range", () => {
    expect(clampSpecPanelWidth(200)).toBe(SPEC_PANEL_MIN_WIDTH);
    expect(clampSpecPanelWidth(2000)).toBe(SPEC_PANEL_MAX_WIDTH);
    expect(clampSpecPanelWidth(640.4)).toBe(640);
  });
});

describe("clampSpecPanelWidthForShell", () => {
  it("lets the panel grow past half the window on a wide shell", () => {
    expect(
      clampSpecPanelWidthForShell(1200, 1920, SPEC_PANEL_MIN_REMAINING_SHELL_WIDTH),
    ).toBe(1200);
  });

  it("never steals the reserved workspace column", () => {
    expect(clampSpecPanelWidthForShell(1400, 1000, /*reservedWidth*/ 360)).toBe(640);
    expect(clampSpecPanelWidthForShell(200, 1000, /*reservedWidth*/ 360)).toBe(SPEC_PANEL_MIN_WIDTH);
  });
});
