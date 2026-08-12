import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Avatar } from "./avatar";
import { getInitials } from "./utils";

describe("Avatar", () => {
    it("renders initials when no profile image is available", () => {
        render(<Avatar initials="AL" />);

        expect(screen.getByText("AL")).toBeVisible();
    });

    it("falls back to initials when the profile image fails", () => {
        render(<Avatar src="/missing-profile.png" alt="Ada Lovelace" initials="AL" />);

        fireEvent.error(screen.getByRole("img", { name: "Ada Lovelace" }));

        expect(screen.queryByRole("img", { name: "Ada Lovelace" })).not.toBeInTheDocument();
        expect(screen.getByText("AL")).toBeVisible();
    });

    it("uses one or two name parts when deriving initials", () => {
        expect([getInitials("Ada"), getInitials("Ada Lovelace")]).toEqual(["A", "AL"]);
    });
});
