# ora-spec

Filesystem adapter that discovers, indexes and watches the spec documents living inside a
workspace.

## Responsibilities

- Resolve which discovery sources apply to a workspace, combining the repository-owned
  `.ora/specs.toml` (or the built-in presets when it is absent) with per-user additions.
- Walk a workspace root, attribute files to sources by glob, and build one catalog entry
  per document with its identity, title and content fingerprint.
- Keep the catalog fresh by watching only the directories that configured patterns can
  reach, debouncing bursts, and rescanning when the watcher reports activity.
- Serve the raw markdown of a catalogued document.

## Non-responsibilities

- No persistence. The catalog is rebuilt from disk and never written to storage.
- No editing. Documents are modified by agents or external editors, never by this crate.
- No provenance or drift detection. This crate produces the content fingerprints that a
  later drift-detection layer will compare, but performs no comparison itself.
- No general-purpose file watching. Only configured spec directories are observed.

## Public boundary

`SpecCatalog` is the entry point. `SpecCatalog::snapshot` returns the current
`SpecSnapshot` for a workspace root, and `SpecCatalog::read_document` returns one
document together with its body. Failures surface as `SpecError`.

Domain types (`SpecDocument`, `SpecSource`, `SpecPath`, `SpecIdentity`,
`SpecContentHash`) belong to `ora-domain`; this crate only produces them.

## Key invariants

- **Freshness is decided by content, never by timestamps.** Editors with autosave and
  format-on-save rewrite files with identical bytes, so a modification time says nothing
  about whether a document actually changed. Every entry carries a SHA-256 fingerprint of
  its bytes, and a rewrite that leaves content untouched leaves the snapshot untouched.
- **Paths are normalized to forward slashes.** A document that declares no identifier is
  identified by its path, so that path must be spelled the same way on every platform.
- **A rename is a delete followed by a create.** Watchers carry no rename semantics, which
  is why a path cannot serve as a durable identity and why the crate reacts to change by
  rescanning rather than by patching individual entries.
- **Overlapping patterns produce one entry.** A file is attributed to the first source, in
  configuration order, whose glob matches it.
- **Reads are whitelisted by the catalog.** `read_document` resolves its argument against
  indexed documents before touching the filesystem, so a path outside the configured
  sources is rejected without a read rather than filtered afterwards.

## Lifecycle

One workspace is indexed at a time. A task backed by a linked worktree is a different
branch with a different set of spec files, and the user reads exactly one workspace at a
time; keeping every visited workspace warm would accumulate operating system watch handles
for branches nobody is looking at. Requesting a snapshot for a different root therefore
releases the previous watcher and indexes the new one.

Within one workspace, a cached snapshot is reused until the watcher reports a change, so
repeated polling costs nothing while the disk is quiet. Reloading also re-reads
`.ora/specs.toml`, so source configuration changes take effect without restarting Ora.

## Failure semantics

- A workspace root that is missing or is not a directory fails the request.
- Malformed source configuration or an invalid glob fails the request, rather than
  degrading to a silently empty catalog.
- An individual file that cannot be read is skipped. One unreadable or binary file must
  not make the whole catalog unavailable.
- Scanning stops at a fixed entry budget so a pattern rooted too broadly cannot walk an
  entire drive.
