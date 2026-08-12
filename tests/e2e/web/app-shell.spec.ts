import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

test("renders the Ora workspace without browser runtime errors", async ({ page }) => {
    const browserErrors: string[] = [];
    page.on("console", (message) => {
        if (message.type() === "error") {
            browserErrors.push(message.text());
        }
    });
    page.on("pageerror", (error) => browserErrors.push(error.message));

    await page.goto("/");

    await expect(page).toHaveTitle("Ora");
    await expect(page.getByRole("main", { name: "Ora workspace" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Ora" })).toBeAttached();
    await expect(page.getByText("EW")).toBeVisible();
    expect(browserErrors).toEqual([]);
});

test("has no automatically detectable WCAG A or AA violations", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("main", { name: "Ora workspace" })).toBeVisible();

    const accessibilityScan = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"]).analyze();

    expect(accessibilityScan.violations).toEqual([]);
});
