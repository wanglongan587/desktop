import { create } from "zustand";

interface SpecPanelState {
  open: boolean;
  /** Workspace-relative path of the document the reader shows, if any. */
  selectedPath: string | null;
  openPanel: () => void;
  closePanel: () => void;
  togglePanel: () => void;
  /** Selects a document inside the already-open panel. */
  selectSpec: (path: string) => void;
  /** Opens the panel on one document, used by entry points outside the panel itself. */
  revealSpec: (path: string) => void;
}

/**
 * Owns the spec panel's disclosure and reading position.
 *
 * The selected path is kept as a plain string rather than a resolved document so
 * the panel can survive a rescan that replaces every document object, and so the
 * chat card can point at a file the catalog has not indexed yet.
 */
export const useSpecPanelStore = create<SpecPanelState>((set) => ({
  open: false,
  selectedPath: null,
  openPanel: () => set({ open: true }),
  closePanel: () => set({ open: false }),
  togglePanel: () => set((state) => ({ open: !state.open })),
  selectSpec: (selectedPath) => set({ selectedPath }),
  revealSpec: (selectedPath) => set({ open: true, selectedPath }),
}));
