import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { SpecPanel } from "./spec-panel";
import {
  SPEC_PANEL_ANIMATION_MS,
  SPEC_PANEL_DEFAULT_WIDTH,
  SPEC_PANEL_MIN_WIDTH,
  clampSpecPanelWidthForShell,
} from "../../lib/spec-panel-layout";

/**
 * Hosts the Spec panel as a right-edge frame that slides open like Codex's
 * secondary sidebar: the outer width animates, while the inner pane keeps a
 * stable width so markdown does not reflow mid-transition. Dragging the left
 * edge resizes within the configured budget; double-click restores the default.
 */
export function SpecPanelFrame() {
  const { t } = useTranslation();
  const open = useSpecPanelStore((state) => state.open);
  const panelWidth = useSpecPanelStore((state) => state.panelWidth);
  const setPanelWidth = useSpecPanelStore((state) => state.setPanelWidth);
  const [contentMounted, setContentMounted] = useState(open);
  const [frameWidth, setFrameWidth] = useState(open ? panelWidth : 0);
  const [animating, setAnimating] = useState(false);
  const [dragging, setDragging] = useState(false);
  const dragStateRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const openRef = useRef(open);
  openRef.current = open;

  useEffect(() => {
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (open) {
      // Fit the remembered width to the current shell so a large saved size cannot
      // swallow the chat column on a smaller window after the user undocks/resizes.
      const targetWidth = clampSpecPanelWidthForShell(
        useSpecPanelStore.getState().panelWidth,
        document.documentElement.clientWidth,
      );
      if (targetWidth !== useSpecPanelStore.getState().panelWidth) {
        setPanelWidth(targetWidth);
      }

      setContentMounted(true);
      if (reduceMotion) {
        setFrameWidth(targetWidth);
        setAnimating(false);
        return;
      }

      // Start from a closed width, then expand on a timeout rather than nested rAFs:
      // Strict Mode cancels animation frames during the mount/remount cycle and can
      // leave the frame stuck at 0px after a chat-card reveal.
      setAnimating(true);
      setFrameWidth(0);
      const openTimer = window.setTimeout(() => {
        if (openRef.current) setFrameWidth(targetWidth);
      }, 16);
      const settleTimer = window.setTimeout(() => setAnimating(false), SPEC_PANEL_ANIMATION_MS + 32);
      return () => {
        window.clearTimeout(openTimer);
        window.clearTimeout(settleTimer);
      };
    }

    if (reduceMotion) {
      setFrameWidth(0);
      setContentMounted(false);
      setAnimating(false);
      return;
    }

    setAnimating(true);
    setFrameWidth(0);
    const timer = window.setTimeout(() => {
      setAnimating(false);
      setContentMounted(false);
    }, SPEC_PANEL_ANIMATION_MS);
    return () => window.clearTimeout(timer);
  }, [open, setPanelWidth]);

  // Keep the animated frame aligned with drags and default resets while open.
  useEffect(() => {
    if (open && !animating) setFrameWidth(panelWidth);
  }, [open, panelWidth, animating]);

  useEffect(() => {
    if (!dragging) return;

    const onPointerMove = (event: PointerEvent) => {
      const drag = dragStateRef.current;
      if (drag === null) return;
      const shellWidth = document.documentElement.clientWidth;
      const nextWidth = drag.startWidth + (drag.startX - event.clientX);
      setPanelWidth(clampSpecPanelWidthForShell(nextWidth, shellWidth));
    };

    const endDrag = () => {
      dragStateRef.current = null;
      setDragging(false);
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", endDrag);
      window.removeEventListener("pointercancel", endDrag);
    };
  }, [dragging, setPanelWidth]);

  if (!contentMounted && !open && frameWidth === 0) {
    return null;
  }

  const transitionEnabled = animating && !dragging;
  const showChrome = frameWidth > 0 || open;

  return (
    <aside
      data-testid="spec-panel-frame"
      className={`relative h-full shrink-0 overflow-hidden bg-sidebar ${
        showChrome ? "border-l border-border" : ""
      } ${
        transitionEnabled
          ? "transition-[width] ease-[cubic-bezier(0.22,1,0.36,1)] motion-reduce:transition-none"
          : ""
      }`}
      style={{
        width: frameWidth,
        transitionDuration: transitionEnabled ? `${SPEC_PANEL_ANIMATION_MS}ms` : undefined,
      }}
      aria-hidden={!open}
    >
      {/*
        Keep the inner pane at the remembered width while the outer clip animates
        from 0 → width. Without this, content would squash during the slide and
        the markdown column would reflow twice for one open.
      */}
      <div className="flex h-full min-h-0" style={{ width: Math.max(panelWidth, SPEC_PANEL_MIN_WIDTH) }}>
        {contentMounted && <SpecPanel />}
      </div>
      {open && (
        <div
          role="separator"
          aria-orientation="vertical"
          aria-label={t("spec.resize")}
          title={t("spec.resize")}
          tabIndex={0}
          className="absolute inset-y-0 left-0 z-20 flex w-3 -translate-x-1/2 cursor-col-resize items-center justify-center outline-none"
          onPointerDown={(event) => {
            event.preventDefault();
            dragStateRef.current = { startX: event.clientX, startWidth: panelWidth };
            setDragging(true);
            setAnimating(false);
            event.currentTarget.setPointerCapture(event.pointerId);
          }}
          onDoubleClick={() => setPanelWidth(SPEC_PANEL_DEFAULT_WIDTH)}
          onKeyDown={(event) => {
            if (event.key === "ArrowLeft") {
              event.preventDefault();
              setPanelWidth(
                clampSpecPanelWidthForShell(
                  panelWidth + 24,
                  document.documentElement.clientWidth,
                ),
              );
            } else if (event.key === "ArrowRight") {
              event.preventDefault();
              setPanelWidth(
                clampSpecPanelWidthForShell(
                  panelWidth - 24,
                  document.documentElement.clientWidth,
                ),
              );
            } else if (event.key === "Home") {
              event.preventDefault();
              setPanelWidth(SPEC_PANEL_DEFAULT_WIDTH);
            }
          }}
        >
          <span
            className={`h-6 w-1 rounded-lg bg-border transition-colors ${
              dragging ? "bg-ring" : "hover:bg-ring"
            }`}
          />
        </div>
      )}
    </aside>
  );
}
