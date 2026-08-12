import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Checkbox } from "./checkbox/checkbox";
import { Input } from "./input/input";
import { NativeSelect } from "./select/select-native";
import { TextArea } from "./textarea/textarea";
import { Toggle } from "./toggle/toggle";

describe("form controls", () => {
    it("changes checkboxes through their accessible labels", async () => {
        const onChange = vi.fn();
        const user = userEvent.setup();
        render(<Checkbox label="Enable notifications" onChange={onChange} />);

        const checkbox = screen.getByRole("checkbox", { name: "Enable notifications" });
        await user.click(checkbox);

        expect(checkbox).toBeChecked();
        expect(onChange).toHaveBeenCalledWith(true);
    });

    it("changes toggles from the keyboard", async () => {
        const onChange = vi.fn();
        const user = userEvent.setup();
        render(<Toggle label="Compact mode" onChange={onChange} />);

        const toggle = screen.getByRole("switch", { name: "Compact mode" });
        toggle.focus();
        await user.keyboard(" ");

        expect(toggle).toBeChecked();
        expect(onChange).toHaveBeenCalledWith(true);
    });

    it("binds input labels, supports typing, and reveals passwords", async () => {
        const user = userEvent.setup();
        render(<Input label="Password" type="password" isRequired />);

        const input = screen.getByLabelText(/Password/);
        expect(input).toHaveAttribute("type", "password");
        expect(input).toBeRequired();

        await user.type(input, "secret");
        await user.click(screen.getByRole("button", { name: "Toggle password visibility" }));

        expect(input).toHaveValue("secret");
        expect(input).toHaveAttribute("type", "text");
    });

    it("binds textarea help text to an editable field", async () => {
        const user = userEvent.setup();
        render(<TextArea label="Description" hint="Describe the task" />);

        const textarea = screen.getByRole("textbox", { name: "Description" });
        await user.type(textarea, "Investigate the failure");

        expect(textarea).toHaveValue("Investigate the failure");
        expect(screen.getByText("Describe the task")).toBeVisible();
    });

    it("selects a native option by its visible label", async () => {
        const user = userEvent.setup();
        render(
            <NativeSelect
                label="Environment"
                options={[
                    { label: "Development", value: "dev" },
                    { label: "Production", value: "prod" },
                    { label: "Retired", value: "retired", disabled: true },
                ]}
            />,
        );

        const select = screen.getByRole("combobox", { name: "Environment" });
        await user.selectOptions(select, "prod");

        expect(select).toHaveValue("prod");
        expect(screen.getByRole("option", { name: "Retired" })).toBeDisabled();
    });
});
