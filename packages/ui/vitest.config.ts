import { defineConfig } from "vitest/config";

export default defineConfig({
    test: {
        environment: "jsdom",
        include: ["src/**/*.test.{ts,tsx}"],
        setupFiles: ["./src/test/setup.ts"],
        reporters: ["default", "junit"],
        outputFile: {
            junit: "../../test-results/components/junit.xml",
        },
        coverage: {
            provider: "v8",
            include: ["src/**/*.{ts,tsx}"],
            exclude: ["src/**/*.test.{ts,tsx}", "src/test/**"],
            reportsDirectory: "../../coverage/ui",
            reporter: ["text", "json-summary", "lcov"],
            thresholds: {
                lines: 14,
                functions: 14,
                statements: 14,
                branches: 14,
            },
        },
    },
});
