import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ComponentPropsWithoutRef,
  type RefObject,
} from "react";
import type { Components } from "react-markdown";
import ReactMarkdown from "react-markdown";
import { markdownComponents, markdownRemarkPlugins } from "../markdown/markdown-core";
import { unwrapMarkdownDocument } from "./markdown-document";
import { prepareStreamingMarkdown } from "./streaming-markdown";

interface MarkdownMessageProps {
  content: string;
  streaming?: boolean;
}

interface MarkdownAstPosition {
  start: { offset?: number };
  end: { offset?: number };
}

interface MarkdownAstNode {
  type: string;
  value?: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: MarkdownAstNode[];
  position?: MarkdownAstPosition;
}

interface StreamingRevealState {
  renderedLength: number;
  revealStart: number;
  streaming: boolean;
}

interface StreamingRevealSpanProps extends ComponentPropsWithoutRef<"span"> {
  node?: MarkdownAstNode;
  registryRef: RefObject<Set<HTMLElement>>;
  "data-stream-text-reveal"?: boolean;
}

/** Renders untrusted assistant Markdown without enabling raw HTML execution. */
export function MarkdownMessage({ content, streaming = false }: MarkdownMessageProps) {
  const markdown = unwrapMarkdownDocument(content);
  const renderedMarkdown = useFrameBatchedMarkdown(markdown, streaming);
  const parseableMarkdown = useMemo(
    () => streaming ? prepareStreamingMarkdown(renderedMarkdown) : renderedMarkdown,
    [renderedMarkdown, streaming],
  );
  const [storedRevealState, setStoredRevealState] = useState<StreamingRevealState>(() => ({
    renderedLength: renderedMarkdown.length,
    revealStart: streaming ? 0 : renderedMarkdown.length,
    streaming,
  }));
  let revealState = storedRevealState;
  if (storedRevealState.renderedLength !== renderedMarkdown.length || storedRevealState.streaming !== streaming) {
    // Chat content is append-only while streaming. Length is therefore enough
    // to retain the prior boundary without rescanning an ever-growing string.
    revealState = {
      renderedLength: renderedMarkdown.length,
      revealStart: streaming && renderedMarkdown.length >= storedRevealState.renderedLength
        ? storedRevealState.renderedLength
        : renderedMarkdown.length,
      streaming,
    };
    setStoredRevealState(revealState);
  }
  const revealPlugin = useMemo(
    () => createStreamingRevealPlugin(revealState.revealStart),
    [revealState.revealStart],
  );
  const revealNodesRef = useRef(new Set<HTMLElement>());
  const streamingMarkdownComponents = useMemo<Components>(() => ({
    ...markdownComponents,
    span: (props) => <StreamingRevealSpan {...props} registryRef={revealNodesRef} />,
  }), []);
  const rehypePlugins = useMemo(
    () => streaming ? [revealPlugin] : [],
    [revealPlugin, streaming],
  );
  const markdownBody = useMemo(
    () => (
      <ReactMarkdown
        remarkPlugins={markdownRemarkPlugins}
        rehypePlugins={rehypePlugins}
        components={streaming ? streamingMarkdownComponents : markdownComponents}
      >
        {parseableMarkdown}
      </ReactMarkdown>
    ),
    [parseableMarkdown, rehypePlugins, streaming, streamingMarkdownComponents],
  );

  useLayoutEffect(() => {
    if (streaming) animateStreamingReveal(revealNodesRef.current);
  }, [renderedMarkdown, streaming]);

  return (
    <div data-selectable className="min-w-0 break-words text-[15px] leading-[26px] text-foreground">
      {markdownBody}
    </div>
  );
}

/** Coalesces stream chunks per frame while guaranteeing progress during continuous output. */
function useFrameBatchedMarkdown(markdown: string, streaming: boolean) {
  const [renderedMarkdown, setRenderedMarkdown] = useState(markdown);
  const latestMarkdownRef = useRef(markdown);
  const committedMarkdownRef = useRef(markdown);
  const frameRef = useRef<number | null>(null);

  useLayoutEffect(() => {
    latestMarkdownRef.current = markdown;
    if (committedMarkdownRef.current === markdown || frameRef.current !== null) return;

    const scheduleFrame = window.requestAnimationFrame
      ?? ((callback: FrameRequestCallback) => window.setTimeout(() => callback(performance.now()), 16));
    frameRef.current = scheduleFrame(() => {
      frameRef.current = null;
      const latestMarkdown = latestMarkdownRef.current;
      committedMarkdownRef.current = latestMarkdown;
      setRenderedMarkdown(latestMarkdown);
    });
  }, [markdown]);

  useEffect(() => () => {
    if (frameRef.current === null) return;
    if (window.cancelAnimationFrame) window.cancelAnimationFrame(frameRef.current);
    else window.clearTimeout(frameRef.current);
  }, []);

  return streaming ? renderedMarkdown : markdown;
}

/** Creates one Markdown transform that marks only newly streamed prose for reveal animation. */
function createStreamingRevealPlugin(revealStart: number) {
  return () => (tree: MarkdownAstNode) => {
    wrapStreamingText(tree, revealStart);
  };
}

/** Wraps text after the previous stream boundary while leaving code structures untouched. */
function wrapStreamingText(node: MarkdownAstNode, revealStart: number) {
  if (node.tagName === "code" || node.tagName === "pre" || node.children === undefined) return;
  for (let index = node.children.length - 1; index >= 0; index -= 1) {
    const child = node.children[index]!;
    const childEndOffset = child.position?.end.offset;
    // Source-ordered HAST lets us stop as soon as we reach stable content,
    // keeping reveal work proportional to the newest streamed suffix.
    if (childEndOffset !== undefined && childEndOffset <= revealStart) break;
    if (child.type !== "text" || child.value === undefined) {
      wrapStreamingText(child, revealStart);
      continue;
    }

    const startOffset = child.position?.start.offset;
    if (startOffset === undefined || childEndOffset === undefined) continue;

    const sourceLength = childEndOffset - startOffset;
    // Entities and escapes can make source offsets diverge from visible text.
    // Revealing the whole node is preferable to hiding freshly streamed text
    // behind an unsafe proportional offset.
    const splitIndex = revealStart <= startOffset || sourceLength !== child.value.length
      ? 0
      : Math.min(child.value.length, revealStart - startOffset);
    const stableText = child.value.slice(0, splitIndex);
    const revealedText = child.value.slice(splitIndex);
    if (revealedText === "") continue;
    const revealNode: MarkdownAstNode = {
      type: "element",
      tagName: "span",
      properties: { dataStreamTextReveal: true },
      children: [{ ...child, value: revealedText }],
    };
    node.children.splice(
      index,
      1,
      ...(stableText === "" ? [revealNode] : [{ ...child, value: stableText }, revealNode]),
    );
  }
}

/** Animates the latest prose batch and releases compositor resources when it settles. */
function animateStreamingReveal(nodes: ReadonlySet<HTMLElement>) {
  if (typeof HTMLElement.prototype.animate !== "function") return;
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  nodes.forEach((node) => {
    node.getAnimations().forEach((animation) => animation.cancel());
    const animation = node.animate(
      [
        { opacity: 0.2 },
        { opacity: 1 },
      ],
      { duration: 180, easing: "cubic-bezier(0.2, 0, 0, 1)" },
    );
    animation.addEventListener("finish", () => animation.cancel(), { once: true });
  });
}

/** Registers one animated Markdown span without rescanning the complete message DOM. */
function StreamingRevealSpan({
  node,
  registryRef,
  "data-stream-text-reveal": reveal,
  ...props
}: StreamingRevealSpanProps) {
  const spanRef = useRef<HTMLSpanElement>(null);
  const shouldReveal = reveal === true || node?.properties?.dataStreamTextReveal === true;

  useLayoutEffect(() => {
    const span = spanRef.current;
    if (!shouldReveal || span === null) return;
    const registry = registryRef.current;
    registry.add(span);
    return () => {
      registry.delete(span);
    };
  }, [registryRef, shouldReveal]);

  return <span ref={spanRef} data-stream-text-reveal={shouldReveal || undefined} {...props} />;
}

