import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const defaultSources = [
    { suite: "rust", format: "junit", path: "target/nextest/ci/junit.xml" },
    { suite: "components", format: "junit", path: "test-results/components/junit.xml" },
    { suite: "web", format: "junit", path: "test-results/web-e2e/junit.xml" },
    { suite: "web", format: "json", path: "test-results/web-e2e/results.json" },
    { suite: "desktop", format: "junit", path: "test-results/desktop-e2e/junit.xml" },
    { suite: "rust-coverage", format: "lcov", path: "coverage/rust/lcov.info" },
    { suite: "ui-coverage", format: "json", path: "coverage/ui/coverage-summary.json" },
    { suite: "runtime", format: "json", path: "runtime-assets/prepared/runtime-manifest.json" },
];

/** Reads a numeric attribute from a JUnit element without depending on a particular test runner. */
function junitNumber(tag, attribute) {
    const match = tag.match(new RegExp(`\\b${attribute}=["']([^"']+)["']`));
    return match ? Number(match[1]) : 0;
}

/** Extracts the aggregate counters needed to triage a JUnit report. */
export function summarizeJunit(contents) {
    const root = contents.match(/<testsuites\b[^>]*>/)?.[0];
    const suites = [...contents.matchAll(/<testsuite\b[^>]*>/g)].map((match) => match[0]);
    const tags = root ? [root] : suites;
    return tags.reduce(
        (summary, tag) => ({
            tests: summary.tests + junitNumber(tag, "tests"),
            failures: summary.failures + junitNumber(tag, "failures"),
            errors: summary.errors + junitNumber(tag, "errors"),
            skipped: summary.skipped + junitNumber(tag, "skipped"),
        }),
        { tests: 0, failures: 0, errors: 0, skipped: 0 },
    );
}

/** Extracts line coverage from LCOV records while preserving the raw report as evidence. */
export function summarizeLcov(contents) {
    let linesFound = 0;
    let linesHit = 0;
    for (const line of contents.split(/\r?\n/)) {
        if (line.startsWith("LF:")) {
            linesFound += Number(line.slice(3));
        } else if (line.startsWith("LH:")) {
            linesHit += Number(line.slice(3));
        }
    }
    return {
        linesFound,
        linesHit,
        linePercent: linesFound === 0 ? 0 : Number(((linesHit / linesFound) * 100).toFixed(2)),
    };
}

/** Returns the current revision without making evidence collection fail outside a Git checkout. */
function resolveRevision(cwd) {
    if (process.env.GITHUB_SHA) {
        return process.env.GITHUB_SHA;
    }
    try {
        return execFileSync("git", ["rev-parse", "HEAD"], {
            cwd,
            encoding: "utf8",
            stdio: ["ignore", "pipe", "ignore"],
        }).trim();
    } catch {
        return "unknown";
    }
}

/** Builds a compact, hash-addressed index over all test reports available in the workspace. */
export function collectEvidence(cwd, sources = defaultSources) {
    const available = [];
    const missing = [];
    for (const source of sources) {
        const absolutePath = resolve(cwd, source.path);
        if (!existsSync(absolutePath)) {
            missing.push(source.path);
            continue;
        }
        const contents = readFileSync(absolutePath);
        const text = contents.toString("utf8");
        const summary = source.format === "junit"
            ? summarizeJunit(text)
            : source.format === "lcov"
                ? summarizeLcov(text)
                : undefined;
        available.push({
            ...source,
            bytes: statSync(absolutePath).size,
            sha256: createHash("sha256").update(contents).digest("hex"),
            ...(summary ? { summary } : {}),
        });
    }
    return {
        schemaVersion: 1,
        revision: resolveRevision(cwd),
        generatedAt: new Date().toISOString(),
        available,
        missing,
    };
}

/** Parses the deliberately small CLI surface so CI jobs can choose their artifact location. */
function parseOutput(arguments_) {
    const outputIndex = arguments_.indexOf("--output");
    if (outputIndex === -1 || !arguments_[outputIndex + 1]) {
        return "test-results/evidence-manifest.json";
    }
    return arguments_[outputIndex + 1];
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : undefined;
if (invokedPath === fileURLToPath(import.meta.url)) {
    const cwd = process.cwd();
    const outputPath = resolve(cwd, parseOutput(process.argv.slice(2)));
    const manifest = collectEvidence(cwd);
    mkdirSync(dirname(outputPath), { recursive: true });
    writeFileSync(outputPath, `${JSON.stringify(manifest, null, 2)}\n`);
    process.stdout.write(`Indexed ${manifest.available.length} evidence files in ${relative(cwd, outputPath)}\n`);
}
