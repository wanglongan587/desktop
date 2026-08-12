import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useClipboard } from "./use-clipboard";

describe("useClipboard", () => {
    beforeEach(() => {
        vi.useFakeTimers();
        Object.defineProperty(window, "isSecureContext", { configurable: true, value: true });
    });

    afterEach(() => {
        vi.useRealTimers();
        vi.restoreAllMocks();
    });

    it("uses the Clipboard API and clears its copied state after the timeout", async () => {
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
        const { result } = renderHook(useClipboard);

        await act(async () => {
            expect(await result.current.copy("hello", "message-1")).toEqual({ success: true });
        });

        expect(writeText).toHaveBeenCalledWith("hello");
        expect(result.current.copied).toBe("message-1");

        act(() => vi.advanceTimersByTime(2_000));
        expect(result.current.copied).toBe(false);
    });

    it("falls back to execCommand when the Clipboard API rejects", async () => {
        Object.defineProperty(navigator, "clipboard", {
            configurable: true,
            value: { writeText: vi.fn().mockRejectedValue(new Error("permission denied")) },
        });
        const execCommand = vi.fn().mockReturnValue(true);
        Object.defineProperty(document, "execCommand", { configurable: true, value: execCommand });
        const { result } = renderHook(useClipboard);

        await act(async () => {
            expect(await result.current.copy("fallback")).toEqual({ success: true });
        });

        expect(execCommand).toHaveBeenCalledWith("copy");
        expect(document.querySelector("textarea")).not.toBeInTheDocument();
        expect(result.current.copied).toBe(true);
    });
});
