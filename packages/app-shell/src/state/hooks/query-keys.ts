/**
 * Centralised react-query cache keys for the app shell.
 *
 * Keeping keys in one place lets mutations invalidate exactly the queries they
 * share data with, without scattering string literals across hook files.
 */
export const queryKeys = {
  projects: ["projects"] as const,
  tasks: ["tasks"] as const,
  sessions: ["sessions"] as const,
  agents: ["agents"] as const,
  skills: ["skills"] as const,
  gitIdentity: ["gitIdentity"] as const,
  agentModels: ["agentModels"] as const,
  // Specs live on disk per workspace, so the workspace legs are part of the key:
  // switching project or task must read a different index, not a stale one.
  specs: (projectId: string, taskId: string | null) => ["specs", projectId, taskId] as const,
  specContent: (projectId: string, taskId: string | null, path: string) =>
    ["specs", projectId, taskId, "content", path] as const,
};

export type WorkspaceQueryKey = readonly ["projects"] | readonly ["tasks"] | readonly ["sessions"];
