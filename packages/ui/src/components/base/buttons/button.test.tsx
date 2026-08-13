import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { Button } from "../../button"

describe("Button", () => {
  it("exposes native button semantics and handles activation", async () => {
    const onClick = vi.fn()
    const user = userEvent.setup()
    render(<Button onClick={onClick}>Save</Button>)

    const button = screen.getByRole("button", { name: "Save" })
    expect(button).toHaveAttribute("type", "button")

    await user.click(button)

    expect(onClick).toHaveBeenCalledOnce()
  })

  it("prevents disabled buttons from being activated", async () => {
    const onClick = vi.fn()
    const user = userEvent.setup()
    render(
      <Button disabled onClick={onClick}>
        Delete
      </Button>
    )

    const button = screen.getByRole("button", { name: "Delete" })
    await user.click(button)

    expect(button).toBeDisabled()
    expect(onClick).not.toHaveBeenCalled()
  })

  it("applies the selected visual variants", () => {
    render(
      <Button variant="outline" size="lg">
        Settings
      </Button>
    )

    expect(screen.getByRole("button", { name: "Settings" })).toHaveAttribute(
      "data-slot",
      "button"
    )
  })
})
