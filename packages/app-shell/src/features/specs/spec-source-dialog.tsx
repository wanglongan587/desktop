import { useEffect, useState } from "react";
import type {
  SpecSource,
  SpecSourceVisibility,
  SpecTarget,
  SpecWorkflow,
} from "@ora/contracts";
import { usePlatform } from "@ora/platform";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Switch,
} from "@ora/ui";
import { IconFolderPlus, IconTrash } from "@tabler/icons-react";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useContractsClient } from "../../contracts-client-context";
import { localizeContractError } from "../../i18n/contract-error";
import { queryKeys } from "../../state/hooks/query-keys";

interface SpecSourceDialogProps {
  open: boolean;
  projectId: string;
  target: SpecTarget;
  initialPath: string | undefined;
  sources: SpecSource[];
  onOpenChange: (open: boolean) => void;
}

/** Edits project-wide source overrides without coupling the form to workflow session state. */
export function SpecSourceDialog({
  open,
  projectId,
  target,
  initialPath,
  sources,
  onOpenChange,
}: SpecSourceDialogProps) {
  const { t } = useTranslation();
  const platform = usePlatform();
  const client = useContractsClient();
  const queryClient = useQueryClient();
  const [rows, setRows] = useState<SpecSource[]>(sources);
  const [busy, setBusy] = useState<"selecting" | "saving" | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setRows(sources);
      setError(null);
    }
  }, [open, sources]);

  const addDirectory = async () => {
    setBusy("selecting");
    setError(null);
    try {
      const absolutePath = await platform.selectPath({ kind: "directory", initialPath });
      if (absolutePath === null) return;
      const resolved = await client.spec.resolveSource({ target, absolutePath });
      setRows((current) => {
        if (current.some((source) => source.relativePath === resolved.relativePath)) {
          return current;
        }
        return [...current, {
          relativePath: resolved.relativePath,
          workflow: resolved.workflow,
          origin: "manual",
          visibility: "enabled",
          availability: "available",
        }];
      });
    } catch (cause) {
      setError(localizeContractError(cause, t));
    } finally {
      setBusy(null);
    }
  };

  const save = async () => {
    setBusy("saving");
    setError(null);
    try {
      await client.spec.updateProjectSources({
        projectId,
        sources: rows.map(({ relativePath, workflow, visibility }) => ({
          relativePath,
          workflow,
          visibility,
        })),
      });
      await queryClient.invalidateQueries({ queryKey: queryKeys.specs(projectId) });
      onOpenChange(false);
    } catch (cause) {
      setError(localizeContractError(cause, t));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Dialog open={open} onOpenChange={(next) => (busy === null || next) && onOpenChange(next)}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t("specs.sourcesTitle")}</DialogTitle>
          <DialogDescription>{t("specs.sourcesDescription")}</DialogDescription>
        </DialogHeader>
        <div className="max-h-[55vh] space-y-2 overflow-y-auto pr-1">
          {rows.map((source, index) => (
            <SourceRow
              key={`${source.relativePath}-${index}`}
              source={source}
              onChange={(next) => setRows((current) => current.map((item, itemIndex) => itemIndex === index ? next : item))}
              onDelete={source.origin === "manual"
                ? () => setRows((current) => current.filter((_, itemIndex) => itemIndex !== index))
                : undefined}
            />
          ))}
          {rows.length === 0 && (
            <p className="rounded-md border border-dashed border-border p-4 text-sm text-muted-foreground">
              {t("specs.noSources")}
            </p>
          )}
        </div>
        {error && <p role="alert" data-selectable className="text-xs text-destructive">{error}</p>}
        <DialogFooter className="sm:justify-between">
          <Button type="button" variant="outline" disabled={busy !== null || initialPath === undefined} onClick={() => void addDirectory()}>
            <IconFolderPlus />
            {t("specs.addDirectory")}
          </Button>
          <div className="flex gap-2">
            <Button type="button" variant="outline" disabled={busy !== null} onClick={() => onOpenChange(false)}>
              {t("common.cancel")}
            </Button>
            <Button type="button" disabled={busy !== null} onClick={() => void save()}>
              {busy === "saving" ? t("common.saving") : t("common.save")}
            </Button>
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function SourceRow({ source, onChange, onDelete }: {
  source: SpecSource;
  onChange: (source: SpecSource) => void;
  onDelete?: () => void;
}) {
  const { t } = useTranslation();
  const workflowValue = source.workflow.kind;
  const workflowEditable = source.origin === "manual";
  const setWorkflow = (kind: string | null) => {
    const workflow: SpecWorkflow = kind === "open_spec"
      ? { kind: "open_spec" }
      : kind === "superpowers"
        ? { kind: "superpowers" }
        : { kind: "custom", name: source.workflow.kind === "custom" ? source.workflow.name : t("specs.custom") };
    onChange({ ...source, workflow });
  };
  const setVisibility = (enabled: boolean) => {
    const visibility: SpecSourceVisibility = enabled ? "enabled" : "disabled";
    onChange({ ...source, visibility });
  };

  return (
    <div className="grid gap-2 rounded-md border border-border p-3 md:grid-cols-[minmax(0,1fr)_150px_auto]">
      <div className="min-w-0">
        <p className="truncate font-mono text-xs">{source.relativePath}</p>
        <p className="mt-1 text-[11px] text-muted-foreground">
          {t(`specs.origin.${source.origin}`)} · {t(`specs.availability.${source.availability}`)}
        </p>
      </div>
      <div className="grid gap-1.5">
        <Select value={workflowValue} disabled={!workflowEditable} onValueChange={setWorkflow}>
          <SelectTrigger className="w-full"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="open_spec">OpenSpec</SelectItem>
            <SelectItem value="superpowers">Superpowers</SelectItem>
            <SelectItem value="custom">{t("specs.custom")}</SelectItem>
          </SelectContent>
        </Select>
        {source.workflow.kind === "custom" && (
          <Input
            aria-label={t("specs.customName")}
            value={source.workflow.name}
            disabled={!workflowEditable}
            onChange={(event) => onChange({ ...source, workflow: { kind: "custom", name: event.target.value } })}
          />
        )}
      </div>
      <div className="flex items-start gap-1">
        <Switch
          checked={source.visibility === "enabled"}
          aria-label={t("specs.toggleSource")}
          onCheckedChange={setVisibility}
        />
        {onDelete && (
          <Button size="icon-sm" variant="ghost" aria-label={t("specs.removeSource")} onClick={onDelete}>
            <IconTrash />
          </Button>
        )}
      </div>
    </div>
  );
}
