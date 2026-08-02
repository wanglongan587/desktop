import { useTranslation } from "react-i18next";
import { Button, Spinner } from "@ora/ui";
import { IconLayoutSidebarRightCollapse } from "@tabler/icons-react";
import { localizeContractError } from "../../i18n/contract-error";
import { useSpecs } from "../../state/hooks/use-specs";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { useWorkspaceSelectionStore } from "../../state/stores/workspace-selection-store";
import { SpecList } from "./spec-list";
import { SpecReader } from "./spec-reader";

/**
 * The third shell column: a spec browser split into a grouped list and a reader.
 *
 * The panel deliberately does not live in the workspace tree. Project/task/session
 * is an execution context, while source/spec is document organization; merging
 * them would overload one tree with two meanings.
 */
export function SpecPanel() {
  const { t } = useTranslation();
  const closePanel = useSpecPanelStore((state) => state.closePanel);
  const selectedPath = useSpecPanelStore((state) => state.selectedPath);
  const selectSpec = useSpecPanelStore((state) => state.selectSpec);
  const projectId = useWorkspaceSelectionStore((state) => state.selection.projectId);
  const { data, error, isPending } = useSpecs();

  const hasWorkspace = projectId !== null;
  const specs = data?.specs ?? [];
  // A path can outlive the document it pointed at: switching workspaces, or an
  // agent deleting the file, both leave the reader aimed at nothing.
  const activePath = specs.some((spec) => spec.path === selectedPath) ? selectedPath : null;

  return (
    <aside className="flex h-full min-h-0 flex-col border-l border-border bg-sidebar" aria-label={t("spec.panel")}>
      <div className="flex h-14 shrink-0 items-center gap-2 px-3">
        <span className="text-[13px] font-medium">{t("spec.title")}</span>
        {specs.length > 0 && (
          <span className="text-[11px] text-muted-foreground tabular-nums">{specs.length}</span>
        )}
        <Button
          variant="ghost"
          size="icon-sm"
          className="ml-auto"
          onClick={closePanel}
          aria-label={t("spec.close")}
        >
          <IconLayoutSidebarRightCollapse />
        </Button>
      </div>
      {!hasWorkspace && (
        <p className="px-3 py-6 text-center text-[13px] text-muted-foreground">{t("spec.pickProject")}</p>
      )}
      {hasWorkspace && error && (
        <p data-selectable className="px-3 py-6 text-[13px] text-destructive">{localizeContractError(error, t)}</p>
      )}
      {hasWorkspace && !error && isPending && (
        <p className="flex items-center justify-center gap-2 px-3 py-6 text-[13px] text-muted-foreground">
          <Spinner className="size-4" />
          {t("spec.loading")}
        </p>
      )}
      {hasWorkspace && !error && data !== undefined && (
        <div className="flex min-h-0 flex-1">
          <div className="min-h-0 w-56 shrink-0 overflow-y-auto border-r border-border">
            <SpecList
              sources={data.sources}
              specs={data.specs}
              selectedPath={activePath}
              onSelect={selectSpec}
            />
          </div>
          <div className="min-h-0 min-w-0 flex-1 overflow-y-auto bg-background">
            <SpecReader path={activePath} />
          </div>
        </div>
      )}
    </aside>
  );
}
