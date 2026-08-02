import { useTranslation } from "react-i18next";
import { IconFileText } from "@tabler/icons-react";
import type { SpecDocument, SpecSource } from "@ora/contracts";

interface SpecListProps {
  sources: readonly SpecSource[];
  specs: readonly SpecDocument[];
  selectedPath: string | null;
  onSelect: (path: string) => void;
}

/**
 * Lists discovered specs grouped by the source that claimed them.
 *
 * Grouping follows the backend's configuration order rather than sorting by name,
 * so the panel mirrors how the team declared its spec layout. Sources without any
 * document are hidden: an empty preset is noise, not information.
 */
export function SpecList({ sources, specs, selectedPath, onSelect }: SpecListProps) {
  const { t } = useTranslation();
  const groups = sources
    .map((source) => ({
      source,
      documents: specs.filter((spec) => spec.sourceName === source.name),
    }))
    .filter((group) => group.documents.length > 0);

  if (groups.length === 0) {
    return (
      <div className="px-3 py-6 text-center text-[13px] text-muted-foreground">
        <p>{t("spec.empty")}</p>
        <p className="mt-2 text-[11px]">{t("spec.emptyHint")}</p>
      </div>
    );
  }

  return (
    <div className="py-2">
      {groups.map(({ source, documents }) => (
        <section key={source.name} className="mb-2">
          <h3 className="flex h-7 items-center gap-2 px-3 text-[11px] font-medium text-muted-foreground">
            <span className="truncate">{source.name}</span>
            <span className="ml-auto shrink-0 tabular-nums">{documents.length}</span>
          </h3>
          {documents.map((document) => (
            <button
              key={document.path}
              type="button"
              onClick={() => onSelect(document.path)}
              aria-current={document.path === selectedPath}
              title={document.path}
              className={`flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-[13px] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring ${
                document.path === selectedPath
                  ? "bg-sidebar-accent text-sidebar-accent-foreground"
                  : "hover:bg-sidebar-accent/70"
              }`}
            >
              <IconFileText className="size-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate">{document.title}</span>
            </button>
          ))}
        </section>
      ))}
    </div>
  );
}
