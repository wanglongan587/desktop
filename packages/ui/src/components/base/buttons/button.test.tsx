import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Button } from "./button";

describe("Button", () => {
    it("exposes native button semantics and handles pointer activation", async () => {
        const onClick = vi.fn();
        const user = userEvent.setup();
        render(<Button onClick={onClick}>Save</Button>);

        const button = screen.getByRole("button", { name: "Save" });
        expect(button).toHaveAttribute("type", "button");

        await user.click(button);

        expect(onClick).toHaveBeenCalledOnce();
    });

    it("prevents disabled buttons and links from being activated", async () => {
        const onButtonClick = vi.fn();
        const onLinkClick = vi.fn();
        const user = userEvent.setup();
        render(
            <>
                <Button isDisabled onClick={onButtonClick}>
                    Delete
                </Button>
                <Button href="/settings" isDisabled onClick={onLinkClick}>
                    Settings
                </Button>
            </>,
        );

        await user.click(screen.getByRole("button", { name: "Delete" }));
        await user.click(screen.getByText("Settings"));

        expect(onButtonClick).not.toHaveBeenCalled();
        expect(onLinkClick).not.toHaveBeenCalled();
        const disabledLink = screen.getByText("Settings").closest("[aria-disabled='true']");
        expect(disabledLink).toBeInstanceOf(HTMLElement);
        expect(disabledLink).not.toHaveAttribute("href");
    });

    it("marks pending actions as busy without dropping their accessible name", () => {
        render(
            <Button isLoading showTextWhileLoading>
                Saving
            </Button>,
        );

        const button = screen.getByRole("button", { name: "Saving" });
        expect(button).toHaveAttribute("data-loading", "true");
        expect(button).toHaveAttribute("aria-disabled", "true");
    });
});
