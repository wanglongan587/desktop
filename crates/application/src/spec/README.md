# Spec Application Module

This module implements the transport-independent use cases for browsing the spec documents that live inside a repository.

## Responsibilities and boundaries

- Listing resolves the workspace a request targets, reads the current catalog, and returns the discovered documents together with the sources that produced them.
- Reading returns one catalogued document and its raw markdown body, resolved through the catalog so a path outside the configured sources is refused before any file is touched.
- Sources are returned even when a workspace has no documents, because the chat surface matches freshly written paths against their globs before the catalog has observed the write.
- Workspace and catalog failures are translated into stable `ApplicationError` variants.

`SpecWorkspaceResolver` and `SpecCatalogReader` isolate workspace resolution and discovery from the handlers, so both use cases are testable without a filesystem. Discovery itself — glob matching, frontmatter parsing, content hashing, and watching — belongs to `ora-spec`, and the scope decision (project root versus a task's own worktree) belongs to the resolver implementation in `ora-backend`.

Nothing in this module writes specs or persists anything about them. The repository files remain the single source of truth, and the catalog is always rebuildable from disk.

See the [ora-application overview](../../README.md).
