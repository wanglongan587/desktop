import { render, screen } from "@testing-library/react"
import userEvent from "@testing-library/user-event"
import { describe, expect, it, vi } from "vitest"

import { Checkbox } from "../checkbox"
import { Input } from "../input"
import { NativeSelect, NativeSelectOption } from "../native-select"
import { Textarea } from "../textarea"
import { Toggle } from "../toggle"

describe("form controls", () => {
  it("changes checkboxes through their accessible labels", async () => {
    const onCheckedChange = vi.fn()
    const user = userEvent.setup()
    render(
      <label>
        <Checkbox onCheckedChange={onCheckedChange} />
        Enable notifications
      </label>
    )

    const checkbox = screen.getByRole("checkbox", {
      name: "Enable notifications",
    })
    await user.click(checkbox)

    expect(checkbox).toBeChecked()
    expect(onCheckedChange).toHaveBeenCalledWith(true, expect.anything())
  })

  it("changes toggles from the keyboard", async () => {
    const onPressedChange = vi.fn()
    const user = userEvent.setup()
    render(
      <Toggle aria-label="Compact mode" onPressedChange={onPressedChange}>
        Compact
      </Toggle>
    )

    const toggle = screen.getByRole("button", { name: "Compact mode" })
    toggle.focus()
    await user.keyboard(" ")

    expect(toggle).toHaveAttribute("aria-pressed", "true")
    expect(onPressedChange).toHaveBeenCalledWith(true, expect.anything())
  })

  it("binds input labels and supports typing", async () => {
    const user = userEvent.setup()
    render(
      <label htmlFor="password">
        Password
        <Input id="password" type="password" required />
      </label>
    )

    const input = screen.getByLabelText("Password")
    await user.type(input, "secret")

    expect(input).toHaveValue("secret")
    expect(input).toHaveAttribute("type", "password")
    expect(input).toBeRequired()
  })

  it("binds textarea help text to an editable field", async () => {
    const user = userEvent.setup()
    render(
      <>
        <label htmlFor="description">Description</label>
        <Textarea id="description" aria-describedby="description-hint" />
        <span id="description-hint">Describe the task</span>
      </>
    )

    const textarea = screen.getByRole("textbox", { name: "Description" })
    await user.type(textarea, "Investigate the failure")

    expect(textarea).toHaveValue("Investigate the failure")
    expect(textarea).toHaveAccessibleDescription("Describe the task")
  })

  it("selects a native option by its visible label", async () => {
    const user = userEvent.setup()
    render(
      <label>
        Environment
        <NativeSelect>
          <NativeSelectOption value="dev">Development</NativeSelectOption>
          <NativeSelectOption value="prod">Production</NativeSelectOption>
          <NativeSelectOption value="retired" disabled>
            Retired
          </NativeSelectOption>
        </NativeSelect>
      </label>
    )

    const select = screen.getByRole("combobox", { name: "Environment" })
    await user.selectOptions(select, "prod")

    expect(select).toHaveValue("prod")
    expect(screen.getByRole("option", { name: "Retired" })).toBeDisabled()
  })
})
