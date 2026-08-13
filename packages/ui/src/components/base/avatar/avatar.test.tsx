import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { Avatar, AvatarFallback, AvatarImage } from "../../avatar"

describe("Avatar", () => {
  it("renders fallback content when no profile image is available", () => {
    render(
      <Avatar>
        <AvatarFallback>AL</AvatarFallback>
      </Avatar>
    )

    expect(screen.getByText("AL")).toBeVisible()
  })

  it("shows fallback content while the profile image is unavailable", () => {
    render(
      <Avatar>
        <AvatarImage src="/missing-profile.png" alt="Ada Lovelace" />
        <AvatarFallback>AL</AvatarFallback>
      </Avatar>
    )

    expect(screen.getByText("AL")).toBeVisible()
    expect(
      screen.queryByRole("img", { name: "Ada Lovelace" })
    ).not.toBeInTheDocument()
  })

  it("exposes the selected size to avatar descendants", () => {
    const { container } = render(
      <Avatar size="lg">
        <AvatarFallback>AL</AvatarFallback>
      </Avatar>
    )

    expect(container.querySelector("[data-slot='avatar']")).toHaveAttribute(
      "data-size",
      "lg"
    )
  })
})
