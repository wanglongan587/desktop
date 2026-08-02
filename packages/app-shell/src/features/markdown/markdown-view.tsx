import { useMemo } from "react";
import ReactMarkdown from "react-markdown";
import { markdownComponents, markdownRemarkPlugins } from "./markdown-core";

interface MarkdownViewProps {
  content: string;
  className?: string;
}

/**
 * Renders a complete Markdown document that is already on disk.
 *
 * Unlike the chat renderer this surface has no stream boundary to track, so it
 * skips frame batching and reveal animation entirely: long documents would pay
 * for per-frame reparsing without ever showing an incremental update.
 */
export function MarkdownView({ content, className }: MarkdownViewProps) {
  const body = useMemo(
    () => (
      <ReactMarkdown remarkPlugins={markdownRemarkPlugins} components={markdownComponents}>
        {content}
      </ReactMarkdown>
    ),
    [content],
  );

  return (
    <div
      data-selectable
      className={`min-w-0 break-words text-[15px] leading-[26px] text-foreground ${className ?? ""}`}
    >
      {body}
    </div>
  );
}
