import { describe, expect, it } from "vitest";
import type { SpecSource } from "@ora/contracts";
import { matchSpecSource, toWorkspaceRelativePath } from "./spec-source-match";

const SOURCES: SpecSource[] = [
  { name: "OpenSpec", glob: "openspec/changes/**/*.md" },
  { name: "Docs", glob: "docs/specs/*.md" },
];

describe("toWorkspaceRelativePath", () => {
  it("strips the workspace root from an absolute path", () => {
    expect(toWorkspaceRelativePath("/ora/docs/specs/design.md", "/ora")).toBe("docs/specs/design.md");
  });

  it("normalizes Windows separators on both the path and the root", () => {
    expect(toWorkspaceRelativePath("D:\\ora\\docs\\specs\\design.md", "D:\\ora\\")).toBe(
      "docs/specs/design.md",
    );
  });

  it("keeps an already-relative path unchanged", () => {
    expect(toWorkspaceRelativePath("docs/specs/design.md", "/ora")).toBe("docs/specs/design.md");
  });

  it("rejects an absolute path belonging to another workspace", () => {
    expect(toWorkspaceRelativePath("/other/docs/specs/design.md", "/ora")).toBeNull();
  });
});

describe("matchSpecSource", () => {
  it("matches nested paths through a double-star segment", () => {
    expect(matchSpecSource("/ora/openspec/changes/add-auth/proposal.md", "/ora", SOURCES)).toEqual(
      SOURCES[0],
    );
  });

  it("matches a double-star segment that spans zero directories", () => {
    expect(matchSpecSource("/ora/openspec/changes/proposal.md", "/ora", SOURCES)).toEqual(SOURCES[0]);
  });

  it("does not let a single star cross a directory boundary", () => {
    expect(matchSpecSource("/ora/docs/specs/nested/design.md", "/ora", SOURCES)).toBeNull();
  });

  it("returns null for a file no source claims", () => {
    expect(matchSpecSource("/ora/src/main.rs", "/ora", SOURCES)).toBeNull();
  });

  // Mirrors the backend scanner, which attributes an overlapping file to the first
  // source in configuration order.
  it("attributes an overlapping path to the first configured source", () => {
    const overlapping: SpecSource[] = [
      { name: "Primary", glob: "docs/**/*.md" },
      { name: "Secondary", glob: "docs/specs/*.md" },
    ];

    expect(matchSpecSource("docs/specs/design.md", "/ora", overlapping)).toEqual(overlapping[0]);
  });
});
