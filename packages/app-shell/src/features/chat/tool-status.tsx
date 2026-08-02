import { IconAlertTriangle, IconBan, IconCheck, IconLoader2 } from "@tabler/icons-react";
import { useTranslation } from "react-i18next";
import type { ChatToolCallStatus } from "@ora/chat";

/** Displays tool state with both iconography and localized text. */
export function ToolStatus({ status, compact = false }: { status: ChatToolCallStatus | undefined; compact?: boolean }) {
  const { t } = useTranslation();
  switch (status) {
    case "completed":
      return <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-emerald-600"><IconCheck className="size-3" />{compact ? <span className="sr-only">{t("chat.toolCompleted")}</span> : t("chat.toolCompleted")}</span>;
    case "failed":
      return <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-destructive"><IconAlertTriangle className="size-3" />{compact ? <span className="sr-only">{t("chat.toolFailed")}</span> : t("chat.toolFailed")}</span>;
    case "cancelled":
      return <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground"><IconBan className="size-3" />{compact ? <span className="sr-only">{t("chat.toolCancelled")}</span> : t("chat.toolCancelled")}</span>;
    case "pending":
      return <span className="shrink-0 text-[11px] text-muted-foreground">{compact ? <span className="sr-only">{t("chat.toolPending")}</span> : t("chat.toolPending")}</span>;
    case "in_progress":
      return <span className="inline-flex shrink-0 items-center gap-1 text-[11px] text-sky-600"><IconLoader2 className="size-3 animate-spin motion-reduce:animate-none" />{compact ? <span className="sr-only">{t("chat.toolRunning")}</span> : t("chat.toolRunning")}</span>;
    case undefined:
      return null;
  }
}
