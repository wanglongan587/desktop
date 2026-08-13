import { act, renderHook } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useIsMobile } from "./use-mobile"

describe("useIsMobile", () => {
  let mediaListener: (() => void) | undefined

  beforeEach(() => {
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      value: 1024,
      writable: true,
    })
    Object.defineProperty(window, "matchMedia", {
      configurable: true,
      value: vi.fn().mockReturnValue({
        addEventListener: vi.fn((_event, listener) => {
          mediaListener = listener
        }),
        matches: false,
        media: "(max-width: 767px)",
        onchange: null,
        removeEventListener: vi.fn(),
      }),
    })
  })

  afterEach(() => {
    mediaListener = undefined
    vi.restoreAllMocks()
  })

  it("reports the initial desktop viewport", () => {
    const { result } = renderHook(useIsMobile)

    expect(window.matchMedia).toHaveBeenCalledWith("(max-width: 767px)")
    expect(result.current).toBe(false)
  })

  it("responds to viewport media-query changes", () => {
    const { result } = renderHook(useIsMobile)

    act(() => {
      window.innerWidth = 600
      mediaListener?.()
    })

    expect(result.current).toBe(true)
  })
})
