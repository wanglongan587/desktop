describe("Ora native desktop shell", () => {
    it("opens one native window and renders the packaged frontend", async () => {
        // The embedded provider exposes the Tauri label as the WebDriver window handle. An
        // explicit switch also prevents the service from probing optional advanced IPC hooks.
        await browser.switchToWindow("main");
        expect(await browser.getTitle()).toBe("Ora");
        expect(await browser.getWindowHandles()).toHaveLength(1);

        const workspace = await $("main[aria-label='Ora workspace']");
        await workspace.waitForDisplayed();
        expect(await workspace.isDisplayed()).toBe(true);

        const heading = await $("h1=Ora");
        expect(await heading.isExisting()).toBe(true);
        expect(await $("main [data-avatar] span").getText()).toBe("EW");
    });
});
