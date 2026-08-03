import { useTranslation } from "react-i18next";
import { Button, Tooltip, TooltipContent, TooltipTrigger } from "@ora/ui";
import { IconFileText } from "@tabler/icons-react";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { useWorkspaceSelectionStore } from "../../state/stores/workspace-selection-store";

/**
 * Always-visible Spec panel control for the workspace header.
 *
 * The sidebar Project-row action is hover-only, so after the panel is closed the
 * user needs a stable control that does not require hunting for a tree-row
 * hover target.
 */
export function SpecPanelToggle() {
  const { t } = useTranslation();
  const projectId = useWorkspaceSelectionStore((state) => state.selection.projectId);
  const open = useSpecPanelStore((state) => state.open);
  const togglePanel = useSpecPanelStore((state) => state.togglePanel);

  if (projectId === null) return null;

  return (
    <Tooltip>
      <TooltipTrigger
        render={(
          <Button
            variant="ghost"
            size="icon"
            aria-label={open ? t("spec.close") : t("spec.open")}
            aria-pressed={open}
            onClick={togglePanel}
            className={open ? "bg-muted text-foreground" : undefined}
          />
        )}
      >
        <IconFileText />
      </TooltipTrigger>
      <TooltipContent>{open ? t("spec.close") : t("spec.open")}</TooltipContent>
    </Tooltip>
  );
}
