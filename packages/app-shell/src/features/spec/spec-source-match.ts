import type { SpecSource } from "@ora/contracts";

/**
 * Normalizes a path the way the backend's `SpecPath` does.
 *
 * Tool calls report host-native paths, so a Windows agent writes backslashes while
 * every configured glob is written with forward slashes.
 */
function normalizeSeparators(path: string): string {
  return path.replace(/\\/g, "/");
}

/**
 * Compiles one glob into a regular expression matching the subset globset supports here.
 *
 * `**` crosses directory boundaries, `*` and `?` stop at a separator, and everything
 * else is literal. Character classes and brace alternation are deliberately not
 * supported: the presets and configuration files use plain directory globs, and a
 * partial implementation of the richer syntax would silently mis-classify paths.
 */
function compileGlob(glob: string): RegExp {
  let pattern = "";
  for (let index = 0; index < glob.length; index += 1) {
    const character = glob[index];
    if (character === "*") {
      if (glob[index + 1] === "*") {
        // `**/` may match zero directories, so the separator is part of the optional group.
        if (glob[index + 2] === "/") {
          pattern += "(?:.*/)?";
          index += 2;
        } else {
          pattern += ".*";
          index += 1;
        }
        continue;
      }
      pattern += "[^/]*";
      continue;
    }
    if (character === "?") {
      pattern += "[^/]";
      continue;
    }
    pattern += character.replace(/[.+^${}()|[\]\\]/g, "\\$&");
  }
  return new RegExp(`^${pattern}$`);
}

const compiledGlobs = new Map<string, RegExp>();

/** Reuses compiled globs because the chat view tests every tool call against every source. */
function globMatcher(glob: string): RegExp {
  const cached = compiledGlobs.get(glob);
  if (cached !== undefined) return cached;
  const compiled = compileGlob(glob);
  compiledGlobs.set(glob, compiled);
  return compiled;
}

/**
 * Converts an absolute or workspace-relative path into a path the globs can match.
 *
 * Returns `null` when an absolute path lies outside the workspace, which is how a
 * tool call touching an unrelated file is rejected before any glob runs.
 */
export function toWorkspaceRelativePath(path: string, workspaceRoot: string): string | null {
  const normalizedPath = normalizeSeparators(path);
  const normalizedRoot = normalizeSeparators(workspaceRoot).replace(/\/+$/, "");
  if (normalizedRoot !== "") {
    const prefix = `${normalizedRoot}/`;
    // Windows agents report drive letters with arbitrary casing; a case-sensitive
    // prefix check would drop Spec cards for the same file the catalog already saw.
    if (
      normalizedPath.startsWith(prefix) ||
      normalizedPath.toLowerCase().startsWith(prefix.toLowerCase())
    ) {
      return normalizedPath.slice(normalizedRoot.length + 1);
    }
  }
  // A path that is already relative has no root prefix to strip, but an absolute
  // path under a different root must not be treated as one.
  return normalizedPath.startsWith("/") || /^[A-Za-z]:\//.test(normalizedPath)
    ? null
    : normalizedPath;
}

/**
 * Returns the source that claims a path, or `null` when no source does.
 *
 * The first match wins so the result matches the backend, which assigns a document
 * to the first source in configuration order that accepts it.
 */
export function matchSpecSource(
  path: string,
  workspaceRoot: string,
  sources: readonly SpecSource[],
): SpecSource | null {
  const relativePath = toWorkspaceRelativePath(path, workspaceRoot);
  if (relativePath === null) return null;
  return sources.find((source) => globMatcher(source.glob).test(relativePath)) ?? null;
}
