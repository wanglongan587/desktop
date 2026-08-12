import { defineConfig, devices } from "@playwright/test";

const webPort = 4173;
const webOrigin = `http://127.0.0.1:${webPort}`;

export default defineConfig({
    testDir: "./tests/e2e/web",
    outputDir: "./test-results/web-e2e/artifacts",
    fullyParallel: true,
    forbidOnly: Boolean(process.env.CI),
    retries: 0,
    timeout: 30_000,
    expect: {
        timeout: 5_000,
    },
    reporter: [
        ["list"],
        ["junit", { outputFile: "test-results/web-e2e/junit.xml" }],
        ["json", { outputFile: "test-results/web-e2e/results.json" }],
        ["html", { open: "never", outputFolder: "playwright-report/web" }],
    ],
    use: {
        baseURL: webOrigin,
        trace: "retain-on-failure",
        screenshot: "only-on-failure",
        video: "retain-on-failure",
    },
    webServer: {
        command: `pnpm --filter @ora/desktop exec vite --host 127.0.0.1 --port ${webPort} --strictPort`,
        url: webOrigin,
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
        stdout: "pipe",
        stderr: "pipe",
    },
    projects: [
        {
            name: "chromium",
            use: { ...devices["Desktop Chrome"] },
        },
    ],
});
