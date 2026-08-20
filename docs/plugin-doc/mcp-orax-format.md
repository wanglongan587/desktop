# Ora MCP `.orax` Package Specification

Status: Ora v1 design specification
Specification version: `1`
Compatibility baselines: MCPB manifest `0.3`; MCP Registry `server.json` `2025-12-11`

This document is the normative specification for MCP packages distributed to Ora as `.orax` archives. It supersedes conflicting MCP package examples in other planning documents. The research and rationale behind these rules are recorded in [MCP Plugin Design Research](mcp-plugin-design-research.md).

## 1. Normative language

The terms **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**, and **MAY** are normative requirements.

Ora v1 means the first production implementation of this specification. A future specification version may add profiles or descriptor versions, but an implementation MUST NOT silently reinterpret a package created for another specification version.

## 2. Goals

This specification defines:

- the `.orax` archive structure for MCP plugins;
- the Ora-specific `orax.toml` manifest;
- a local stdio profile compatible with MCPB `0.3`;
- a remote Streamable HTTP profile compatible with the MCP Registry `server.json` schema dated `2025-12-11`;
- identity, version, input, secret, runtime, installation, and persistence invariants;
- the typed descriptor passed from Ora to Agent plugins;
- the materialization and receipt semantics required for reliable Agent configuration.

The design keeps Ora-specific metadata thin. Ora reuses official MCP ecosystem schemas where they already express the required semantics, and adds stricter profile constraints where Ora requires a single, deterministic runtime shape.

## 3. Non-goals

Ora v1 does not define or promise:

- direct double-click installation of `.orax` files by MCPB hosts;
- MCPB compatibility for Streamable HTTP packages;
- the deprecated HTTP+SSE transport;
- bundled local Streamable HTTP servers;
- multiple endpoints, fallback transports, or runtime transport selection;
- arbitrary install scripts or dependency installation hooks;
- automatic `npm install`, `pip install`, or similar package-manager execution;
- an Ora-owned OAuth client, browser authorization flow, or token refresh loop;
- universal hot-plug of MCP servers into already running Agent sessions;
- direct database access by Agent plugins;
- static installation validation as proof that an MCP server can initialize successfully.

## 4. Terminology

### 4.1 Package

A **package** is one `.orax` ZIP archive containing one MCP plugin version.

### 4.2 Marketplace release metadata

The **marketplace release metadata** is the registry-side record that identifies a downloadable `.orax` asset, its exact version, SHA-256 digest, publisher, supported platform, and Ora host requirement. It is not stored inside the archive.

### 4.3 Ora manifest

The **Ora manifest** is the root `orax.toml`. It defines Ora identity, plugin kind, descriptor profile, host compatibility, and permission intent.

### 4.4 Descriptor

The **descriptor** is the official ecosystem manifest selected by the Ora profile:

- `manifest.json` for `mcpb-stdio`;
- `server.json` for `registry-remote`.

### 4.5 Compiled descriptor

An **InstalledMcpDescriptor** is Ora's immutable, Agent-independent representation produced after archive, manifest, descriptor, path, input, and identity validation.

### 4.6 Resolved descriptor

A **ResolvedMcpForAgent** is a Session-time representation in which Ora has resolved the current platform, immutable installation root, runtime executable, workspace, and input references. It is still independent of any target Agent configuration syntax.

### 4.7 Materialization

**Materialization** is the Agent plugin operation that converts a resolved descriptor set into the target Agent's project/worktree configuration.

### 4.8 Workspace scope

A **workspace scope** is the stable identity of one Agent configuration destination:

```text
McpWorkspaceScope
├── ProjectRoot { project_id }
└── Worktree { worktree_id }
```

It is not a Session ID and is not an absolute filesystem path.

## 5. Compatibility profiles

Ora v1 supports exactly two MCP package profiles.

| Ora profile       | Descriptor      | Required descriptor version | Transport                           | Compatibility claim                                                |
| ----------------- | --------------- | --------------------------- | ----------------------------------- | ------------------------------------------------------------------ |
| `mcpb-stdio`      | `manifest.json` | MCPB `0.3`                  | bundled local stdio                 | MCPB archive, strict manifest, and launch-semantics compatible     |
| `registry-remote` | `server.json`   | MCP Registry `2025-12-11`   | one remote Streamable HTTP endpoint | compatible with the pinned Registry descriptor subset defined here |

Ora MUST model these profiles as a closed enum. A package MUST select exactly one profile. A package MUST NOT express a transport as a free-form string combined with unrelated optional fields.

Compatibility applies to packages accepted by the Ora profile. Ora may reject an otherwise schema-valid MCPB or Registry descriptor when it violates the additional security and determinism requirements in this specification, such as secret interpolation in arguments, insecure remote endpoints, or multiple transport alternatives.

The public compatibility statement SHOULD be:

> Ora MCP `.orax` provides two strict profiles. The stdio profile is compatible with MCPB 0.3 archive, manifest, and launch semantics. The remote HTTP profile is compatible with the versioned MCP Registry `server.json` description for one Streamable HTTP remote. Both profiles add an Ora-specific `orax.toml`.

## 6. Archive format

### 6.1 Container

A `.orax` package MUST be a ZIP archive. The `.orax` extension identifies the Ora container; changing only the extension of an arbitrary `.mcpb` file does not create a valid Ora package because `orax.toml` is required.

### 6.2 Root files

Every package MUST contain a root `orax.toml`.

An `mcpb-stdio` package MUST contain a root `manifest.json`.

A `registry-remote` package MUST contain a root `server.json`.

The selected descriptor filename MUST exactly match the filename declared in `orax.toml`. Descriptor paths MUST NOT point into subdirectories.

### 6.3 Example stdio package

```text
github-mcp.orax
├── orax.toml
├── manifest.json
├── server/
├── node_modules/ or lib/
├── assets/
├── icon.png
└── README.md
```

### 6.4 Example remote HTTP package

```text
github-remote.orax
├── orax.toml
├── server.json
├── assets/
├── icon.png
└── README.md
```

### 6.5 Additional files

Additional regular files MAY be included when they are required by the selected profile or are static documentation/assets. Their presence MUST NOT change the meaning of `orax.toml` or the selected descriptor.

An Ora installer MUST NOT execute a package-provided install, post-install, activation, or migration script.

### 6.6 Archive safety

The installer MUST treat every archive as untrusted input, including archives referenced by the official marketplace.

The installer MUST reject:

- absolute archive paths;
- Windows drive-qualified or UNC paths;
- `..` traversal after normalization;
- paths that escape the staging or final installation root;
- symbolic links, hard links, junctions, reparse points, device files, sockets, or other special files;
- two archive entries that normalize to the same path;
- descriptor or entry-point paths that escape the package root;
- archives exceeding configured limits for compressed size, entry count, single-file expanded size, total expanded size, path length, or nesting depth.

The CLI packer and production installer MUST use the same archive/path validation implementation and the same default limits. Limits MUST be finite and exposed in developer diagnostics. Generic path and archive logic belongs in `ora-utils::path` and `ora-utils::archive`.

## 7. `orax.toml`

### 7.1 Strict parsing

`orax.toml` MUST be UTF-8 TOML. The v1 parser MUST reject unknown fields, duplicate keys, unsupported enum values, leading/trailing whitespace in identity fields, and unsupported `schema_version` values.

Ora MUST NOT normalize an invalid ID into a valid one. Package identity is compared using its validated exact spelling.

### 7.2 Common schema

The v1 schema is:

```toml
schema_version = 1
id = "official/github-mcp"
kind = "mcp"
requires_ora = ">=1.0.0"

[mcp]
profile = "mcpb-stdio"
descriptor = "manifest.json"
descriptor_schema = "mcpb:0.3"

[permissions]
network = ["api.github.com"]
filesystem_read = ["workspace"]
filesystem_write = []
```

### 7.3 Field rules

| Field                   | Required | Rule                                                    |
| ----------------------- | -------- | ------------------------------------------------------- |
| `schema_version`        | yes      | MUST equal integer `1`                                  |
| `id`                    | yes      | Ora canonical plugin ID                                 |
| `kind`                  | yes      | MUST equal `mcp`                                        |
| `requires_ora`          | yes      | valid SemVer requirement                                |
| `mcp.profile`           | yes      | `mcpb-stdio` or `registry-remote`                       |
| `mcp.descriptor`        | yes      | exact root descriptor filename for the selected profile |
| `mcp.descriptor_schema` | yes      | pinned schema enum spelling for the selected profile    |
| `permissions`           | yes      | explicit permission intent; empty arrays are allowed    |

`orax.toml` MUST NOT contain a package version. The selected descriptor is the only package-internal version authority.

### 7.4 Canonical Ora ID

The canonical ID is:

```text
<namespace>/<name>
```

Ora v1 supports only the `official` namespace.

The name:

- MUST contain one or two dot-separated slug segments;
- MUST be no more than 128 UTF-8 bytes in total;
- MUST use segments of 1 to 63 ASCII bytes;
- MUST contain only lowercase ASCII letters, digits, and single hyphens;
- MUST NOT begin or end with a hyphen;
- MUST NOT contain consecutive hyphens.

Examples:

```text
official/github-mcp
official/acme.github-mcp
```

GitHub repository names, transfers, and renames MUST NOT implicitly change the Ora ID.

### 7.5 Descriptor profile fields

For stdio, the following values are REQUIRED:

```toml
[mcp]
profile = "mcpb-stdio"
descriptor = "manifest.json"
descriptor_schema = "mcpb:0.3"
```

For remote HTTP, the following values are REQUIRED:

```toml
[mcp]
profile = "registry-remote"
descriptor = "server.json"
descriptor_schema = "mcp-registry:2025-12-11"
```

`descriptor_schema` MUST be parsed as a closed Ora enum, not as a URL to fetch at installation time.

### 7.6 Permission intent

The v1 permission manifest declares user-visible intent. It is not, by itself, proof that a target Agent can enforce the permission.

`permissions.network` entries MUST be host or host-and-port targets without URL schemes, paths, credentials, or wildcards.

`permissions.filesystem_read` and `permissions.filesystem_write` entries MUST use Ora-defined symbolic scopes. V1 recognizes:

- `workspace`;
- `user-selected`;
- `plugin-data`;
- `package` for read intent only.

`filesystem_write` MUST NOT contain `package`, because installed version directories are immutable.

For `registry-remote`, Ora MUST derive and display the actual endpoint network intent from `server.json`; it MUST NOT rely on a duplicated `permissions.network` entry as the endpoint authority. For stdio, declared intent MUST be displayed before activation; enforcement depends on the process/Agent sandbox available to the selected Agent.

## 8. `mcpb-stdio` profile

### 8.1 Strict MCPB baseline

`manifest.json` MUST validate against the vendored official MCPB `0.3` strict schema. Ora MUST pin the schema and compatibility fixtures in its release; it MUST NOT resolve MCPB `latest` at package installation time.

The compatibility implementation SHOULD be tested against the official MCPB source snapshot recorded by the design research, rather than an unpinned branch.

### 8.2 Required launch semantics

Ora MUST preserve MCPB `0.3` semantics for:

- `server.type`;
- `server.entry_point`;
- `server.mcp_config.command`;
- `server.mcp_config.args`;
- `server.mcp_config.env`;
- `server.mcp_config.platform_overrides`;
- MCPB variable substitution;
- `user_config` declarations and placements.

Ora v1 supports the MCPB server types:

```text
node
python
binary
```

Unknown or future server types MUST be rejected under this profile.

### 8.3 Dependencies

Node and Python dependencies required by the MCP server MUST be included according to MCPB `0.3` packaging semantics. Ora MUST NOT run dependency installation commands from the package.

If a required Node or Python runtime is unavailable, the package MAY remain installed with `NeedsRuntime` readiness but MUST NOT be activated or materialized until the runtime is available.

### 8.4 Platform overrides

Ora MUST select the current platform override using MCPB platform names and semantics. Override command/args replace the base values where specified; environment values merge according to MCPB `0.3` behavior.

The compiled descriptor SHOULD preserve the portable base declaration. Session-time resolution MUST produce a concrete current-platform launch description.

### 8.5 Package paths

Every package-relative entry point, command path, argument path, icon path, and referenced asset MUST be normalized and validated against the immutable package root.

For a bundled binary on Windows, Ora MUST apply the pinned MCPB executable suffix behavior. A relative binary command MUST resolve to a regular file inside the installed package version.

### 8.6 stdio rules

The target Agent launches the resolved process and communicates through standard input/output according to the MCP stdio transport. Ora does not launch the MCP process during static package validation.

The MCP process MUST NOT write non-MCP data to stdout. Diagnostics belong on stderr, consistent with the MCP stdio transport.

## 9. `registry-remote` profile

### 9.1 Strict Registry baseline

`server.json` MUST validate against the official versioned schema:

```text
https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json
```

Ora MUST vendor and pin the validator. The installer MUST NOT download a draft or current schema to decide package acceptance.

### 9.2 Ora v1 profile constraints

After strict Registry validation, Ora MUST enforce all of the following:

- `remotes` contains exactly one item;
- the item type is exactly `streamable-http`;
- `packages` is absent or empty;
- deprecated `sse` remotes are rejected;
- multiple endpoint and fallback configurations are rejected;
- the descriptor version is a valid exact SemVer version.

These restrictions are an Ora profile subset. They do not change the official Registry schema.

### 9.3 Endpoint security

The remote URL MUST use HTTPS by default.

Plain HTTP MAY be accepted only through an explicit developer/local-trust host policy and only for loopback endpoints using `localhost`, `127.0.0.1`, or `::1`. A package MUST NOT be able to enable this exception by declaring a manifest field.

If the Registry descriptor uses URL template variables, every resolved endpoint used for materialization MUST satisfy the same HTTPS or explicit loopback policy. A template MUST NOT be treated as authorization to connect to an arbitrary insecure host.

Ora v1 does not support a bundled local HTTP daemon. A remote descriptor MUST NOT contain a local command, entry point, or readiness/shutdown policy.

### 9.4 Authentication

Ora v1 supports:

- endpoints with no authentication;
- Registry header inputs backed by plain or secret input bindings;
- Agent-managed OAuth only when the selected Agent plugin explicitly advertises that capability.

Ora v1 does not own an OAuth authorization client or token refresh lifecycle. OAuth tokens MUST NOT be stored in package files or ordinary SQLite values.

A target Agent that cannot safely express a required header secret or OAuth flow MUST report an item-local unsupported result. The Session may start in `Degraded` state if all global materialization invariants still hold.

## 10. Manifest authority and cross-validation

### 10.1 Authority table

| Information                                                                       | Authority                    |
| --------------------------------------------------------------------------------- | ---------------------------- |
| Ora canonical ID, kind, host requirement, profile, permission intent              | package `orax.toml`          |
| MCPB name, display metadata, author, stdio runtime/config, `user_config`, version | `manifest.json`              |
| Registry name, display metadata, remote transport/config, variables, version      | `server.json`                |
| release asset URL, SHA-256, platform, publisher identity                          | marketplace release metadata |
| active version and installation readiness                                         | Ora database                 |
| actual Agent configuration result                                                 | materialization receipt      |

### 10.2 Identity rules

The marketplace Ora ID MUST equal `orax.toml.id` exactly.

The marketplace kind MUST equal `mcp`, and `orax.toml.kind` MUST equal `mcp`.

MCPB `name` and Registry `name` are upstream identities. They MUST be preserved but MUST NOT replace the Ora canonical ID. They are not required to use the Ora ID spelling.

### 10.3 Version rules

The package descriptor version MUST be an exact SemVer version.

The marketplace release version MUST equal the descriptor version exactly.

`orax.toml` MUST NOT duplicate the version.

### 10.4 Digest rules

The downloaded archive SHA-256 MUST equal the marketplace digest before extraction is trusted.

The tuple `(Ora ID, version)` is immutable. Reinstalling the same tuple with the same archive digest is idempotent. Reinstalling it with a different digest MUST fail with an immutable-version conflict.

### 10.5 No precedence fallback

Any cross-file conflict MUST reject installation. An implementation MUST NOT repair a mismatch by silently preferring the market, Ora manifest, or descriptor value.

## 11. Inputs and secrets

### 11.1 Declaration authority

The selected descriptor declares inputs:

- MCPB `user_config` for stdio;
- Registry variables and header inputs for remote HTTP.

`orax.toml` MUST NOT duplicate input declarations.

Ora MUST compile descriptor input declarations into a common typed model. At minimum it MUST preserve the input ID, user-facing description, value kind, required flag, sensitivity, allowed/default values when permitted, numeric constraints, multiplicity, and placements.

### 11.2 MCPB input kinds

Ora v1 MUST support the MCPB `0.3` input kinds required by the pinned schema, including string, number, boolean, file, and directory forms.

### 11.3 Secret restrictions

A secret input:

- MUST NOT have a package-provided default value;
- MUST NOT be interpolated into a command or argument;
- MAY be placed only into an environment variable or HTTP header secret reference;
- MUST NOT be stored as plaintext in the archive, SQLite, a project/worktree file, a receipt, or logs;
- MUST NOT be returned to an Agent plugin as a plaintext value when a safe reference mechanism is required.

A Registry header declared as secret MUST NOT carry a fixed plaintext header value in the package.

Actual secret values MUST be stored in an operating-system credential store. Ora SQLite stores only opaque references or the information required to request a safe Agent-specific reference.

### 11.4 Input scopes

Input bindings use the precedence:

```text
non-secret descriptor default
  < user global binding
  < ProjectRoot/Worktree binding
```

Workspace overrides use the same `ProjectRoot | Worktree` identity as MCP selection. Session-local bindings are not part of v1.

### 11.5 Missing inputs

A structurally valid package with unresolved required inputs MAY be installed as `NeedsInput`. It MUST NOT be activated or materialized until required inputs are resolved.

If a required input or secret becomes unavailable before Session creation, Ora MUST return a structured `McpConfigurationRequired` result before calling the Agent adapter. This precondition blocks Session creation; it is not an item-local Agent failure.

## 12. Installation and compilation

### 12.1 Installation directory

Installed versions use:

```text
<app-data>/plugins/installed/<namespace>/<name>/<version>/
```

Every version directory is immutable after successful installation. Active version selection is stored in the database; Ora v1 MUST NOT depend on a `current` symlink.

### 12.2 Installation transaction

The installer MUST perform:

```text
download to same-filesystem staging
  -> verify marketplace SHA-256
  -> safely extract
  -> strictly parse orax.toml
  -> strictly validate selected descriptor
  -> cross-validate identity/version/profile
  -> compile InstalledMcpDescriptor
  -> determine input/runtime readiness
  -> atomically move to immutable version directory
  -> commit installation records in one SQLite transaction
```

The installer MUST NOT parse or execute package code before archive and manifest validation succeeds.

### 12.3 Crash reconciliation

Filesystem rename and SQLite commit cannot share one transaction. Startup reconciliation MUST handle:

- abandoned staging directories;
- a final immutable directory without a database record;
- a database record whose immutable directory is missing;
- a descriptor/digest mismatch between disk and database.

An unrecoverable mismatch MUST make the version unavailable for materialization. Ora MUST NOT guess or silently rewrite immutable package contents.

### 12.4 Compiled result

Installation produces exactly one variant:

```text
InstalledMcpDescriptor
├── BundledStdio {
│     ora_id,
│     upstream_name,
│     exact_version,
│     portable_launch,
│     input_declarations,
│     permission_intent,
│     package_relative_paths
│   }
└── RemoteStreamableHttp {
      ora_id,
      upstream_name,
      exact_version,
      endpoint_template,
      header_inputs,
      variable_declarations,
      permission_intent
    }
```

A stdio descriptor MUST NOT contain a remote URL or HTTP headers. A remote descriptor MUST NOT contain a local command or executable. Illegal combinations MUST be rejected before this type is constructed.

### 12.5 Static validity versus health

Successful installation proves archive, schema, identity, version, path, and static policy validity. It does not prove:

- a remote endpoint is reachable;
- credentials are accepted;
- a local process starts;
- MCP initialize negotiation succeeds;
- advertised tools are usable.

Health is established at first use or by a separate explicit check.

## 13. Runtime resolution

### 13.1 Responsibility

Ora owns MCPB and Registry parsing, common path security, platform selection, input binding resolution, and runtime discovery. Agent plugins own target-Agent configuration syntax and file semantics.

An Agent plugin MUST NOT open Ora SQLite, reparse the archive, or independently reinterpret MCPB/Registry templates.

### 13.2 Two-stage resolution

Installation compiles a portable descriptor. Before materialization, Ora resolves it against the current machine and workspace.

For stdio, resolution includes:

- applying the current platform override;
- resolving validated package-relative paths against the immutable installation root;
- resolving the runtime executable;
- checking executable existence and declared runtime compatibility;
- producing concrete arguments and safe input references.

For remote HTTP, no executable runtime resolution occurs.

### 13.3 Runtime lookup

Node and Python runtime lookup uses this order:

```text
Ora-bundled compatible runtime
  -> user-explicit compatible runtime
  -> controlled system PATH lookup
  -> NeedsRuntime
```

The resolved descriptor passed to the Agent plugin MUST contain an absolute executable path. The Agent plugin MUST NOT silently select another runtime.

### 13.4 Runtime drift

If a previously available runtime disappears or no longer satisfies the requirement:

- the installed version readiness becomes `NeedsRuntime`;
- affected materialization receipts become `Outdated`;
- new materialization is blocked;
- already running Agent sessions are not forcibly terminated by this rule alone.

## 14. Persistence model

Ora uses the existing application SQLite database and versioned migrations. MCP state MUST NOT be stored in the existing global `plugin_state` or non-sensitive `user_config` tables.

### 14.1 Required concepts

The persistence implementation MUST represent:

- immutable installed package versions and digests;
- the active version for an Ora MCP ID;
- input bindings and secret references;
- Workspace desired MCP selections and revisions;
- current per-Agent materialization receipts;
- per-MCP applied/failed receipt items;
- the exact materialization revision and MCP versions loaded by each Session.

Suggested table responsibilities are:

```text
installed_mcp_packages
mcp_input_bindings
workspace_mcp_selections
agent_mcp_materializations
agent_mcp_materialization_items
session_mcp_loads
```

Exact SQL belongs to Ora database migrations, not the archive wire format.

### 14.2 Workspace desired selection

All conversations rooted at the same project root share one desired MCP selection. All conversations rooted at the same worktree share another selection. Different worktrees are independent.

Workspace selection supports:

```text
McpVersionPolicy
├── FollowActive
└── Pinned { exact_version }  # reserved/supported when exposed by product UI
```

Materialization and Session load records MUST always resolve this policy to an exact version.

### 14.3 Materialization receipt identity

The current receipt is keyed by:

```text
(workspace_scope, agent_plugin_id)
```

It is not keyed by Session. Multiple Sessions using the same Agent in one workspace share the target Agent configuration.

Ora v1 stores only the current receipt, not an append-only materialization history. Per-MCP results MUST remain individually queryable for uninstall and cleanup; they MUST NOT exist only in an opaque JSON blob.

### 14.4 Receipt contents

The receipt MUST record at least:

- desired revision;
- materialization revision;
- Agent plugin identity and version;
- applied MCP IDs and exact versions;
- failed MCP IDs, exact versions, failure phases, and structured reasons;
- stable managed entry identities;
- a configuration fingerprint;
- materialization status and local timestamp.

The receipt MUST NOT contain plaintext secrets or a rendered configuration containing plaintext secrets.

### 14.5 Session load record

A Session MUST record the desired revision, materialization revision, and exact applied MCP versions it actually loaded. This is distinct from the current Workspace receipt, which may change while the Session continues running.

## 15. Agent materialization interface

This section defines the required integration seam. Individual Agent config formats remain Agent plugin responsibilities.

### 15.1 Request

Ora sends a complete desired set, not incremental enable/disable events:

```text
prepareSessionMcps {
  session_id,
  workspace: {
    scope,
    root
  },
  desired_revision,
  mcps: [ResolvedMcpForAgent]
}
```

The Agent plugin MUST treat this request as an idempotent reconcile operation.

### 15.2 Database isolation

Ora queries its repositories and pushes typed DTOs to the Agent plugin. An Agent plugin MUST NOT receive an Ora database path or issue SQL queries against Ora storage.

### 15.3 Managed entries

The Agent plugin MUST preserve user-created Agent configuration. It may modify or remove only entries it can identify as Ora-managed.

Managed identity MUST be deterministic from stable inputs such as Agent plugin identity and Ora MCP canonical ID. A random ID that exists only in a previous receipt is insufficient for crash recovery.

If a user-owned entry conflicts with the reserved Ora identity and cannot be safely distinguished, materialization MUST fail instead of overwriting the user entry.

### 15.4 Atomic file update

The Agent plugin SHOULD:

1. read and validate the existing target configuration;
2. identify user and Ora-managed entries;
3. build an independent plan for every desired MCP;
4. merge successful plans into one target document;
5. write and validate a temporary file in the target directory;
6. atomically replace the target file;
7. return the receipt and fingerprint.

The Agent plugin MUST NOT report an MCP as applied before the target configuration update succeeds.

### 15.5 Partial failure

Ora permits item-local failures. A successful response may therefore contain both applied and failed items and produce `Degraded` status.

Examples of item-local failures include:

- unsupported transport for the selected Agent;
- unsupported secure secret-reference representation;
- one descriptor that cannot be rendered for that Agent version.

The following are global failures and MUST block Session start:

- the target configuration cannot be read or parsed safely;
- user-owned and Ora-owned entries cannot be distinguished;
- the file cannot be atomically updated;
- the workspace or desired revision changes during materialization;
- the returned fingerprint/receipt cannot be trusted;
- a required Ora input or secret is missing before the Agent call.

When a desired item fails under a new revision, the Agent plugin MUST remove the old Ora-managed entry for that item. It MUST NOT silently keep an old version and claim the new desired version was applied.

### 15.6 Concurrency

Ora MUST serialize materialization for the same:

```text
(workspace_scope, agent_plugin_id)
```

The lock covers configuration inspection, planning, atomic replacement, and receipt persistence. Different workspaces or different Agent plugins MAY materialize concurrently.

### 15.7 Receipt persistence ordering

The Agent plugin writes the file first and returns a receipt; Ora persists that receipt afterward. Ora MUST NOT persist an applied receipt before the file operation succeeds.

A crash after file replacement but before SQLite commit leaves a stale receipt. The next inspect/reconcile MUST recover by reading deterministic managed entries and comparing the configuration fingerprint.

### 15.8 Inspect and no-op reconciliation

Before Session create/load/resume, Ora calls the Agent adapter's inspect/reconcile path. If the normalized configuration and fingerprint already match the current receipt and desired revision, the adapter SHOULD return `AlreadyMaterialized` without rewriting the file.

## 16. State and version lifecycle

### 16.1 Installed version readiness

An installed package version may be structurally valid while not ready for activation.

```text
McpReadiness
├── Ready
├── NeedsInput { missing_input_ids }
└── NeedsRuntime { runtime_requirement }
```

Only `Ready` versions may be newly activated or materialized.

### 16.2 Version role

An immutable installed version has one role:

```text
InstalledVersionRole
├── Available
├── Active
├── Retained
└── PendingRemoval
```

Installation of a valid new version creates `Available`; Ora v1 requires explicit user activation. Static validation MUST NOT automatically change all workspaces.

### 16.3 Activation

Activating a version:

- changes the active exact version for the Ora ID;
- advances every affected `FollowActive` workspace desired revision;
- marks current affected receipts `Outdated`;
- leaves already running Sessions on their loaded exact versions;
- marks affected running Sessions `PendingRestart`.

### 16.4 Materialization status

```text
MaterializationStatus
├── Unknown
├── Ready
├── Degraded
├── Outdated
├── Drifted
└── Blocked
```

- `Ready`: every desired MCP was applied.
- `Degraded`: one or more desired MCP items failed locally, no global invariant failed, and the Session is allowed to start with the successfully applied subset, which may be empty.
- `Outdated`: desired version/configuration, Agent plugin version, or resolver input changed.
- `Drifted`: the target Agent configuration fingerprint no longer matches.
- `Blocked`: a global materialization invariant failed.

### 16.5 Input and secret changes

Changing a global or workspace input/secret binding advances every affected workspace configuration revision, marks receipts `Outdated`, and marks running Sessions `PendingRestart`. An already running MCP may still hold the old environment or authentication state until restart.

### 16.6 Rollback

If a newly active version fails materialization, Ora MUST NOT silently substitute an old version. The UI MAY offer explicit rollback. Rollback changes the active exact version, advances affected desired revisions, and performs a new reconcile.

### 16.7 Old-version retention

An old version MUST NOT be physically removed while it is:

- active;
- referenced by a pinned workspace;
- present in an applied receipt;
- held by a running Session usage lease;
- inside the configured rollback retention window.

## 17. Uninstall compatibility requirements

Full uninstall is outside the initial MCP definition implementation, but v1 records MUST support a staged future flow:

```text
Installed
  -> PendingRemoval
  -> Draining
  -> Removing
  -> Absent
```

`PendingRemoval` immediately prevents new selection/materialization. Ora then removes the MCP from desired selections, reconciles known Agent configurations using receipt managed-entry indexes, waits for running usage leases, and finally deletes the immutable package version.

Ora MUST NOT delete a binary before removing or disabling config entries that reference it. Temporarily inaccessible workspaces require a durable cleanup tombstone and cleanup on next access.

## 18. Validation and error semantics

Validators and installers MUST return structured failures with a stable category, phase, and field/path when applicable. At minimum, implementations must distinguish:

| Category                       | Example                                                  |
| ------------------------------ | -------------------------------------------------------- |
| archive safety                 | traversal, special entry, size limit                     |
| Ora manifest syntax/schema     | invalid TOML, unknown field, unsupported version         |
| descriptor schema              | invalid MCPB/Registry document                           |
| unsupported profile            | unknown profile/schema/transport/server type             |
| identity conflict              | market ID differs from `orax.toml.id`                    |
| version conflict               | market and descriptor versions differ                    |
| digest conflict                | downloaded digest mismatch or immutable-version conflict |
| input policy                   | secret default or secret in command/args                 |
| endpoint policy                | insecure non-loopback HTTP                               |
| runtime readiness              | compatible runtime unavailable                           |
| configuration required         | missing required plain/secret binding                    |
| Agent item unsupported         | transport/auth/reference not representable               |
| materialization global failure | unsafe config read/merge/write/revision                  |

Logs MUST redact secrets and SHOULD avoid absolute workspace paths unless a dedicated user-facing diagnostic explicitly requires them.

## 19. Developer tooling

Ora SHOULD provide one CLI implementation shared with production validation:

```text
orax validate <directory-or-archive>
orax pack <directory>
orax inspect <archive>
```

`validate` MUST perform the same strict profile, descriptor, identity, path, input, and archive validation as the installer, except for checks requiring marketplace metadata or a target machine runtime.

`pack` MUST produce a deterministic-enough archive for repeatable digest generation and MUST refuse unsafe or unsupported files before writing the final `.orax` file.

`inspect` SHOULD display:

- Ora and upstream identities;
- exact version;
- profile and descriptor schema;
- runtime requirements;
- input declarations without values;
- permission intent;
- supported platforms;
- validation warnings/errors.

## 20. Conformance tests

An Ora v1 implementation MUST include tests for:

- official valid MCPB `0.3` fixtures for node, python, and binary;
- official valid Registry `2025-12-11` Streamable HTTP fixtures;
- strict rejection of unknown Ora manifest fields and schema versions;
- profile/descriptor filename and schema mismatches;
- market/package ID, kind, version, and digest mismatches;
- same ID/version with a different digest;
- multiple HTTP remotes, SSE, and packages/remotes fallback rejection;
- HTTPS and loopback HTTP policy;
- archive traversal, absolute paths, drive/UNC paths, links/reparse points, duplicate normalized paths, and expansion limits;
- MCPB platform overrides and Windows executable resolution;
- secret defaults and secret command/argument placement rejection;
- secret absence from SQLite, logs, receipts, and rendered shared project config;
- `NeedsInput` and `NeedsRuntime` readiness;
- crash reconciliation between filesystem promotion and SQLite commit;
- deterministic managed identity and user-entry preservation;
- serialized materialization for one workspace/Agent key;
- `Ready`, `Degraded`, `Outdated`, `Drifted`, and `Blocked` transitions;
- Session exact-version load records and old-version usage protection.

Tests SHOULD compare complete typed objects and receipts rather than asserting individual fields where a deep equality assertion is available.

## 21. References

- [MCPB manifest specification](https://github.com/modelcontextprotocol/mcpb/blob/main/MANIFEST.md)
- [MCPB repository](https://github.com/modelcontextprotocol/mcpb)
- [MCP Registry remote server documentation](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/registry/remote-servers.mdx)
- [MCP Registry `server.json` schema, 2025-12-11](https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json)
- [MCP stdio transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio)
- [MCP Streamable HTTP transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http)
- [Ora MCP plugin design research](mcp-plugin-design-research.md)
