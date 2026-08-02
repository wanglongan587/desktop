import { useTranslation } from "react-i18next";
import { Spinner } from "@ora/ui";
import { localizeContractError } from "../../i18n/contract-error";
import { useSpecContent } from "../../state/hooks/use-specs";
import { MarkdownView } from "../markdown/markdown-view";

interface SpecReaderProps {
  path: string | null;
}

/**
 * Renders the selected spec as read-only markdown.
 *
 * Editing stays out of this phase on purpose: a spec is a repository file, and the
 * agent or an external editor remains its single writer, so the panel never has to
 * reconcile its own buffer against a change arriving from the watcher.
 */
export function SpecReader({ path }: SpecReaderProps) {
  const { t } = useTranslation();
  const { data, error, isPending } = useSpecContent(path);

  if (path === null) {
    return (
      <p className="p-6 text-[13px] text-muted-foreground">{t("spec.selectHint")}</p>
    );
  }
  if (error) {
    return (
      <p data-selectable className="p-6 text-[13px] text-destructive">
        {localizeContractError(error, t)}
      </p>
    );
  }
  if (isPending) {
    return (
      <p className="flex items-center gap-2 p-6 text-[13px] text-muted-foreground">
        <Spinner className="size-4" />
        {t("spec.loading")}
      </p>
    );
  }

  return (
    <article className="px-6 py-5">
      <p data-selectable className="mb-4 truncate font-mono text-[11px] text-muted-foreground" title={data.spec.path}>
        {data.spec.path}
      </p>
      <MarkdownView content={data.content} />
    </article>
  );
}
