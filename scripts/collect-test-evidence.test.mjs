import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { collectEvidence, summarizeJunit, summarizeLcov } from "./collect-test-evidence.mjs";

test("summarizeJunit reads aggregate counters from the root suite", () => {
    const report = '<testsuites tests="7" failures="1" errors="2" skipped="3"><testsuite tests="7"/></testsuites>';
    assert.deepEqual(summarizeJunit(report), { tests: 7, failures: 1, errors: 2, skipped: 3 });
});

test("summarizeLcov aggregates multiple source-file records", () => {
    const report = "LF:10\nLH:8\nend_of_record\nLF:5\nLH:3\nend_of_record\n";
    assert.deepEqual(summarizeLcov(report), { linesFound: 15, linesHit: 11, linePercent: 73.33 });
});

test("collectEvidence hashes available reports and records missing reports", () => {
    const root = mkdtempSync(join(tmpdir(), "ora-evidence-"));
    try {
        mkdirSync(join(root, "reports"));
        writeFileSync(join(root, "reports", "junit.xml"), '<testsuites tests="2" failures="0" errors="0" skipped="1"/>');
        const sources = [
            { suite: "sample", format: "junit", path: "reports/junit.xml" },
            { suite: "sample", format: "json", path: "reports/missing.json" },
        ];

        const manifest = collectEvidence(root, sources);

        assert.deepEqual(manifest.available, [
            {
                suite: "sample",
                format: "junit",
                path: "reports/junit.xml",
                bytes: 59,
                sha256: "18d3318f3b86dfc5aa664901a8477ae26e814647081b7bdacea324e2995d5b0a",
                summary: { tests: 2, failures: 0, errors: 0, skipped: 1 },
            },
        ]);
        assert.deepEqual(manifest.missing, ["reports/missing.json"]);
        assert.match(manifest.generatedAt, /^\d{4}-\d{2}-\d{2}T/);
    } finally {
        rmSync(root, { recursive: true, force: true });
    }
});
