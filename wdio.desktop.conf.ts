import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

const evidenceDirectory = resolve("test-results", "desktop-e2e");
const applicationBinary = resolve("target", "debug", "ora-desktop.exe");

export const config: WebdriverIO.Config = {
    runner: "local",
    outputDir: evidenceDirectory,
    specs: ["./tests/e2e/desktop/**/*.spec.ts"],
    maxInstances: 1,
    capabilities: [
        {
            browserName: "tauri",
            "tauri:options": {
                application: applicationBinary,
            },
        },
    ],
    services: [
        [
            "tauri",
            {
                appBinaryPath: applicationBinary,
                driverProvider: "embedded",
                embeddedPort: 4445,
                startTimeout: 90_000,
                statusPollTimeout: 10_000,
                captureBackendLogs: true,
            },
        ],
    ],
    logLevel: "info",
    bail: 0,
    waitforTimeout: 10_000,
    connectionRetryTimeout: 90_000,
    connectionRetryCount: 0,
    framework: "mocha",
    reporters: [
        "spec",
        [
            "junit",
            {
                outputDir: evidenceDirectory,
                outputFileFormat: () => "junit.xml",
            },
        ],
    ],
    mochaOpts: {
        ui: "bdd",
        timeout: 60_000,
        retries: 0,
    },
    afterTest: async function (_test, _context, result) {
        if (!result.passed) {
            mkdirSync(resolve(evidenceDirectory, "screenshots"), { recursive: true });
            await browser.saveScreenshot(resolve(evidenceDirectory, "screenshots", "failure.png"));
        }
    },
};
