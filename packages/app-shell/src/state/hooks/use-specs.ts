import { useQuery } from "@tanstack/react-query";
import type { ListSpecsResponse, ReadSpecResponse } from "@ora/contracts";
import { useContractsClient } from "../../contracts-client-context";
import { useWorkspaceSelectionStore } from "../stores/workspace-selection-store";
import { useSpecPanelStore } from "../stores/spec-panel-store";
import { queryKeys } from "./query-keys";

/**
 * How often an open panel re-reads the catalog.
 *
 * The backend watches the configured spec directories and only rebuilds its index
 * when file contents actually change, so this poll is cheap: it exists to carry
 * an already-detected change across to the UI, not to discover the change.
 */
const SPEC_POLL_INTERVAL_MS = 4000;

/** Resolves the workspace the spec catalog should be read for. */
function useSpecWorkspace() {
  const selection = useWorkspaceSelectionStore((state) => state.selection);
  return { projectId: selection.projectId, taskId: selection.taskId };
}

/**
 * Loads the spec catalog for the workspace the user is currently looking at.
 *
 * Selecting a task reads that task's workspace, which for worktree-backed tasks is
 * a different branch holding a different set of files; selecting only a project
 * reads the project root.
 */
export function useSpecs() {
  const client = useContractsClient();
  const { projectId, taskId } = useSpecWorkspace();
  const panelOpen = useSpecPanelStore((state) => state.open);

  return useQuery<ListSpecsResponse>({
    queryKey: queryKeys.specs(projectId ?? "", taskId),
    queryFn: () => {
      if (projectId === null) throw new Error("spec catalog requires a selected project");
      return client.spec.list({ projectId, taskId: taskId ?? undefined });
    },
    enabled: projectId !== null,
    refetchInterval: panelOpen ? SPEC_POLL_INTERVAL_MS : false,
  });
}

/**
 * Loads the markdown body of one spec, keyed by workspace so branches never share a cache entry.
 *
 * The body is polled on the same interval as the catalog because an agent rewriting
 * the open document changes its contents without changing its path, which would
 * otherwise leave the reader showing a stale body indefinitely.
 */
export function useSpecContent(path: string | null) {
  const client = useContractsClient();
  const { projectId, taskId } = useSpecWorkspace();

  return useQuery<ReadSpecResponse>({
    queryKey: queryKeys.specContent(projectId ?? "", taskId, path ?? ""),
    queryFn: () => {
      if (projectId === null || path === null) throw new Error("spec content requires a selected document");
      return client.spec.read({ projectId, taskId: taskId ?? undefined, path });
    },
    enabled: projectId !== null && path !== null,
    refetchInterval: SPEC_POLL_INTERVAL_MS,
  });
}
