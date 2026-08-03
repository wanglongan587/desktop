import { create } from "zustand";
import {
  SPEC_PANEL_DEFAULT_WIDTH,
  clampSpecPanelWidth,
} from "../../lib/spec-panel-layout";

interface SpecPanelState {
  open: boolean;
  /** Remembered pixel width of the right-hand Spec frame while it stays open. */
  panelWidth: number;
  /** Workspace-relative path of the document the reader shows, if any. */
  selectedPath: string | null;
  openPanel: () => void;
  closePanel: () => void;
  togglePanel: () => void;
  setPanelWidth: (width: number) => void;
  /** Selects a document inside the already-open panel. */
  selectSpec: (path: string) => void;
  /** Opens the panel on one document, used by entry points outside the panel itself. */
  revealSpec: (path: string) => void;
}

/**
 * Owns the spec panel's disclosure, width, and reading position.
 *
 * The selected path is kept as a plain string rather than a resolved document so
 * the panel can survive a rescan that replaces every document object, and so the
 * chat card can point at a file the catalog has not indexed yet. Width is session
 * memory only: a drag should stick for the current shell lifetime without writing
 * a new persistence surface.
 */
export const useSpecPanelStore = create<SpecPanelState>((set) => ({
  open: false,
  panelWidth: SPEC_PANEL_DEFAULT_WIDTH,
  selectedPath: null,
  openPanel: () => set({ open: true }),
  closePanel: () => set({ open: false }),
  togglePanel: () => set((state) => ({ open: !state.open })),
  setPanelWidth: (width) => set({ panelWidth: clampSpecPanelWidth(width) }),
  selectSpec: (selectedPath) => set({ selectedPath }),
  revealSpec: (selectedPath) => set({ open: true, selectedPath }),
}));
