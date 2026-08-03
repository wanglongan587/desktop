import { useTranslation } from "react-i18next";
import { IconFileText } from "@tabler/icons-react";
import type { ChatToolCall } from "@ora/chat";
import type { ListSpecsResponse, SpecSource } from "@ora/contracts";
import { useQueryClient } from "@tanstack/react-query";
import { useSpecs } from "../../state/hooks/use-specs";
import { queryKeys } from "../../state/hooks/query-keys";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { useWorkspaceSelectionStore } from "../../state/stores/workspace-selection-store";
import { matchSpecSource, toWorkspaceRelativePath } from "./spec-source-match";
import { ToolStatus } from "../chat/tool-status";

interface SpecCardProps {
  tool: ChatToolCall;
  sourceName: string;
  path: string;
}

/**
 * Decides whether a tool call wrote a spec, and to which source it belongs.
 *
 * Matching happens against the catalog's own source globs rather than a hardcoded
 * list of directories, so a repository that configured its own layout gets the same
 * treatment as one on the presets. Pure so the chat turn can also keep Spec writes
 * out of the generic "changes" compression group before they reach this card.
 */
export function resolveSpecToolCall(
  tool: ChatToolCall,
  catalog: ListSpecsResponse | undefined,
): { sourceName: string; path: string } | null {
  const writesFile = tool.toolKind === "edit";
  const location = tool.locations.at(-1);
  if (!writesFile || location === undefined || catalog === undefined) return null;

  const source: SpecSource | null = matchSpecSource(
    location.path,
    catalog.workspaceRoot,
    catalog.sources,
  );
  if (source === null) return null;
  const relativePath = toWorkspaceRelativePath(location.path, catalog.workspaceRoot);
  return relativePath === null ? null : { sourceName: source.name, path: relativePath };
}

/**
 * Decides whether a tool call wrote a spec, and to which source it belongs.
 *
 * Matching happens against the catalog's own source globs rather than a hardcoded
 * list of directories, so a repository that configured its own layout gets the same
 * treatment as one on the presets. The check runs on the tool call itself because
 * the backend index has not observed the write yet at the moment it is rendered.
 */
export function useSpecToolCall(tool: ChatToolCall): { sourceName: string; path: string } | null {
  const { data } = useSpecs();
  return resolveSpecToolCall(tool, data);
}

/**
 * Surfaces a spec an agent just wrote as an openable card instead of a raw diff.
 *
 * This is also the point where phase two will record provenance: the session, the
 * spec, and the moment it was written are all in hand right here.
 */
export function SpecCard({ tool, sourceName, path }: SpecCardProps) {
  const { t } = useTranslation();
  const revealSpec = useSpecPanelStore((state) => state.revealSpec);
  const projectId = useWorkspaceSelectionStore((state) => state.selection.projectId);
  const taskId = useWorkspaceSelectionStore((state) => state.selection.taskId);
  const queryClient = useQueryClient();
  const fileName = path.split("/").at(-1) ?? path;

  const openSpec = () => {
    revealSpec(path);
    // The card appears from the tool-call stream before the watcher has indexed the
    // write; refresh the catalog as soon as the user asks to read it.
    if (projectId !== null) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.specs(projectId, taskId) });
    }
  };

  return (
    <button
      type="button"
      onClick={openSpec}
      aria-label={t("spec.openCard", { name: fileName })}
      className="flex w-full items-center gap-2 rounded-r-sm border-l-2 border-violet-500/60 bg-muted/25 px-3 py-2 text-left text-xs outline-none transition-colors hover:bg-muted/45 focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/50"
      title={path}
    >
      <IconFileText className="size-4 shrink-0 text-violet-600" />
      <span className="shrink-0 text-[10px] font-medium text-muted-foreground">{t("spec.cardLabel")}</span>
      <span className="min-w-0 flex-1 truncate font-medium">{fileName}</span>
      <span className="shrink-0 text-[10px] text-muted-foreground">{sourceName}</span>
      <ToolStatus status={tool.status} compact />
    </button>
  );
}
