import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Spinner } from "@ora/ui";
import { IconLayoutSidebarRightCollapse } from "@tabler/icons-react";
import { localizeContractError } from "../../i18n/contract-error";
import { useSpecs } from "../../state/hooks/use-specs";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { useWorkspaceSelectionStore } from "../../state/stores/workspace-selection-store";
import { SpecList } from "./spec-list";
import { SpecReader } from "./spec-reader";
import { SPEC_PANEL_COMPACT_BREAKPOINT } from "../../lib/spec-panel-layout";

/**
 * The third shell column: a spec browser split into a grouped list and a reader.
 *
 * The panel deliberately does not live in the workspace tree. Project/task/session
 * is an execution context, while source/spec is document organization; merging
 * them would overload one tree with two meanings. Below the compact breakpoint the
 * list collapses into a picker so the markdown column keeps the full panel width.
 */
export function SpecPanel() {
  const { t } = useTranslation();
  const closePanel = useSpecPanelStore((state) => state.closePanel);
  const selectedPath = useSpecPanelStore((state) => state.selectedPath);
  const selectSpec = useSpecPanelStore((state) => state.selectSpec);
  const projectId = useWorkspaceSelectionStore((state) => state.selection.projectId);
  const { data, error, isPending } = useSpecs();
  const rootRef = useRef<HTMLElement | null>(null);
  const [compact, setCompact] = useState(false);

  useEffect(() => {
    const element = rootRef.current;
    if (element === null) return;

    const observer = new ResizeObserver((entries) => {
      const width = entries[0]?.contentRect.width ?? 0;
      setCompact(width > 0 && width < SPEC_PANEL_COMPACT_BREAKPOINT);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const hasWorkspace = projectId !== null;
  const specs = data?.specs ?? [];
  // Keep a freshly revealed path even before the watcher indexes it: chat cards
  // open the panel on a write the catalog may not have observed yet. Once the
  // catalog loads without that path (deleted / wrong workspace), clear the aim.
  const activePath =
    selectedPath !== null &&
    (data === undefined ||
      isPending ||
      specs.some((spec) => spec.path === selectedPath))
      ? selectedPath
      : null;

  return (
    <aside
      ref={rootRef}
      className="flex h-full min-h-0 w-full min-w-0 flex-col bg-sidebar"
      aria-label={t("spec.panel")}
    >
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
      {hasWorkspace && !error && data !== undefined && compact && (
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <label className="shrink-0 border-b border-border px-3 py-2">
            <span className="sr-only">{t("spec.selectHint")}</span>
            <select
              className="h-9 w-full min-w-0 rounded-md border border-border bg-background px-2 text-[13px] outline-none focus-visible:ring-2 focus-visible:ring-ring"
              value={activePath ?? ""}
              onChange={(event) => {
                if (event.target.value !== "") selectSpec(event.target.value);
              }}
            >
              <option value="" disabled>
                {t("spec.selectHint")}
              </option>
              {data.sources.map((source) => {
                const sourceSpecs = specs.filter((spec) => spec.sourceName === source.name);
                if (sourceSpecs.length === 0) return null;
                return (
                  <optgroup key={source.name} label={source.name}>
                    {sourceSpecs.map((spec) => (
                      <option key={spec.path} value={spec.path}>
                        {spec.title}
                      </option>
                    ))}
                  </optgroup>
                );
              })}
            </select>
          </label>
          <div className="min-h-0 min-w-0 flex-1 overflow-y-auto bg-background">
            <SpecReader path={activePath} />
          </div>
        </div>
      )}
      {hasWorkspace && !error && data !== undefined && !compact && (
        <div className="flex min-h-0 min-w-0 flex-1">
          <div className="min-h-0 w-[min(14rem,38%)] shrink-0 overflow-y-auto border-r border-border">
            <SpecList
              sources={data.sources}
              specs={data.specs}
              selectedPath={activePath}
              onSelect={selectSpec}
            />
          </div>
          <div className="min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden bg-background">
            <SpecReader path={activePath} />
          </div>
        </div>
      )}
    </aside>
  );
}
