# ACP Transport, Framing, and Real-Agent Behavior — Research Findings

**Date:** 2026-07-21
**Scope:** Resolve the 6 open ACP design questions for the Ora B2 plugin-integration spec, against primary sources. The #1 blocker was the wire framing/transport for JSON-RPC over stdio (Ora's B2 doc chose "newline-delimited" but flagged it UNVERIFIED).

---

## TL;DR / Verdicts

| # | Question | Verdict |
|---|----------|---------|
| **1 (BLOCKER)** | Wire framing/transport | **HOLDS.** The authoritative ACP spec *defines* a stdio transport and it is **newline-delimited JSON-RPC** (`\n`-separated, no embedded newlines). **NOT** LSP-style `Content-Length`. Ora's B2 "newline-delimited" choice is correct. |
| **2** | `ClientCapabilities` for a coding agent | **All client capabilities are OPTIONAL (SHOULD advertise, not MUST).** `fs/*` and `terminal/*` are opt-in. `ClientCapabilities::default()` in Ora = `fs{read=false,write=false}`, `terminal=false` — confirmed. Advertising them false makes the agent treat them as unsupported and **degrade** (not refuse). Ora as an IDE **should** advertise `fs.read+write=true` and `terminal=true` if it implements them. |
| **3** | Baseline vs opt-in methods | **Baseline (MUST):** `initialize`, `session/new`, `session/prompt`, `session/cancel`, `session/update`. **Opt-in (MAY):** `session/load`, `session/resume`, `session/close`, `session/list`, `session/delete`, `session/set_mode`, `session/set_config_option`, `fs/*`, `terminal/*`, `logout`. Matches Ora's `literals.rs` + `initialization.rs` exactly. |
| **4** | `session/load` replay semantics | **Replay is via `session/update` notifications BEFORE the response, not in the response body.** v1 response body is `{"result": null}` (Ora's `LoadSessionResponse` adds `modes`/`config_options` — a harmless superset). **v2 removes `session/load` entirely**; replay moved to `session/resume` via a `replayFrom` cursor. Ora's DTOs are v1-shaped. |
| **5** | `session/close` vs `session/ended` | **There is NO `session/ended` method or update variant.** The only terminal session method is `session/close`, opt-in via `sessionCapabilities.close`. Confirmed in spec + Ora's `literals.rs` + `SessionUpdate` enum. |
| **6** | Real agent launch + protocol version | **ProtocolVersion = `1`.** Claude Code and Codex do **NOT** speak ACP natively — they are wrapped by **adapter binaries**: `claude-agent-acp` (Node, npm `@agentclientprotocol/claude-agent-acp`) and `codex-acp` (npm `@zed-industries/codex-acp`, migrating to `@agentclientprotocol/codex-acp`). **There is no `claude --acp` flag.** Ora's `Bun.spawn` must launch the adapter, not the bare CLI. OpenCode is registry-listed as ACP-native but its exact launch command is UNVERIFIED in this pass. |

---

## [BLOCKER 1] Wire framing / transport — RESOLVED, B2 choice holds

### Spec text (authoritative)

The ACP spec lives in the `agentclientprotocol/agent-client-protocol` repo under `docs/protocol/{v1,v2}/`. The transport section is `transports.mdx`:

> ACP uses JSON-RPC to encode messages. JSON-RPC messages **MUST** be UTF-8 encoded.
>
> The protocol currently defines the following transport mechanisms for agent-client communication:
> 1. **stdio**, communication over standard in and standard out
> 2. _Streamable HTTP (draft proposal in progress)_
>
> Agents and clients **SHOULD** support stdio whenever possible.
>
> ## stdio
> In the **stdio** transport:
> - The client launches the agent as a subprocess.
> - The agent reads JSON-RPC messages from its standard input (`stdin`) and sends messages to its standard output (`stdout`).
> - Messages are individual JSON-RPC requests, notifications, or responses.
> - **Messages are delimited by newlines (`\n`), and MUST NOT contain embedded newlines.**
> - The agent **MAY** write UTF-8 strings to its standard error (`stderr`) for logging purposes.
> - The agent **MUST NOT** write anything to its `stdout` that is not a valid ACP message.
> - The client **MUST NOT** write anything to the agent's `stdin` that is not a valid ACP message.

— `docs/protocol/v1/transports.mdx` ([gh api path](https://github.com/agentclientprotocol/agent-client-protocol/blob/main/docs/protocol/v1/transports.mdx)). `v2/transports.mdx` is identical, adding that v2 messages may also be **batch arrays** and a "JSON-RPC Batch Messages" subsection (JSON-RPC 2.0 batch rules).

The spec also explicitly says custom transports are possible ("The protocol is transport-agnostic and can be implemented over any communication channel…"), but for stdio — the transport Ora uses — the framing is **fixed**: newline-delimited, no `Content-Length` header.

### Reference implementation corroboration

The higher-level runtime crate `agent-client-protocol` (source: `agentclientprotocol/rust-sdk`, per the schema repo's README) implements stdio framing exactly as the spec mandates:

- `src/agent-client-protocol/src/stdio.rs`: reads stdin with `BufReader::new(stdin).lines()` (line-by-line, splits on `\n`) and writes stdout via `crate::jsonrpc::write_line(&mut writer, line)`. The `Stdio` struct's `connect_to` delegates to either a `Lines` sink/source (with debug callback) or `ByteStreams`.
- `src/agent-client-protocol/src/jsonrpc/transport_actor.rs`: `parse_incoming_line(line: &str) -> ParsedIncomingLine` — each incoming stdin line is parsed as one JSON value (single message, batch array, or malformed). Confirms one-message-per-line.

No `Content-Length` header parsing anywhere in the reference runtime. This is unambiguous.

### Real-agent corroboration

`zed-industries/claude-agent-acp` `src/index.ts`:

> // stdout is used to send messages to the client
> // we redirect everything else to stderr to make sure it doesn't interfere with ACP
> console.log = console.error;
> console.info = console.error;
> …

This is exactly the spec's "agent MUST NOT write anything to stdout that is not a valid ACP message" — the adapter redirects all non-ACP logging to stderr. Consistent with newline-delimited JSON on stdout.

### Verdict

**Ora's B2 "newline-delimited JSON-RPC over stdio" choice is correct and must not change.** The plugin-sdk ACP tool should:
1. Spawn the agent (adapter) as a subprocess with stdin/stdout pipes.
2. Write each `JsonRpcMessage` as one line of compact JSON + `\n`. **Compact JSON** (no pretty-printing) is required because the spec forbids embedded newlines; Ora's `serde_json` serialization must use `compact` (not `pretty`).
3. Read stdout line-by-line (`\n`-delimited), parsing each non-empty line as one JSON-RPC message (or a v2 batch array).
4. Capture stderr for logs but never parse it as ACP.

**The fear that "Claude Code derives from LSP conventions and might use `Content-Length`" is unfounded.** ACP is a clean break from LSP framing; neither the spec, the reference Rust runtime, nor the Claude adapter use `Content-Length`.

---

## [2] ClientCapabilities + agent→client requests

### Spec text

`docs/protocol/v1/initialization.mdx` ("Capabilities" section):

> All capabilities included in the `initialize` request are **OPTIONAL**. Clients and Agents **SHOULD** support all possible combinations of their peer's capabilities.
>
> The introduction of new capabilities is not considered a breaking change. Therefore, Clients and Agents **MUST** treat all capabilities omitted in the `initialize` request as **UNSUPPORTED**.

The "Client Capabilities" subsection reads: "The Client **SHOULD** specify whether it supports the following capabilities" — i.e. SHOULD, not MUST. The fields are:

- `fs.readTextFile` (boolean) — `fs/read_text_file` is available.
- `fs.writeTextFile` (boolean) — `fs/write_text_file` is available.
- `terminal` (boolean) — all `terminal/*` methods are available.
- `session.configOptions.boolean` — client supports boolean session config options.

### Ora's DTOs (confirmation)

`crates/contracts/src/acp/initialization.rs`:

- `ClientCapabilities` (lines 134–160): `fs: FileSystemCapabilities`, `terminal: bool`, `session: Option<ClientSessionCapabilities>`. `#[derive(Default)]` → all false/empty.
- `FileSystemCapabilities` (lines 273–282): `read_text_file: bool`, `write_text_file: bool`, both `#[serde(default)]`.
- `InitializeRequest::new(protocol_version)` (lines 38–44) sets `client_capabilities: ClientCapabilities::default()` — so by default Ora advertises **fs=false, terminal=false**. Confirmed.

The agent→client method surface (methods the *client* must handle if it advertised the capability) is in `crates/contracts/src/acp/literals.rs` `CLIENT_METHOD_NAMES` (lines 111–121): `session/update`, `session/request_permission`, `fs/write_text_file`, `fs/read_text_file`, `terminal/create`, `terminal/output`, `terminal/release`, `terminal/wait_for_exit`, `terminal/kill`.

### Behavior when `fs=false`/`terminal=false`

The spec says clients/agents "**SHOULD** support all possible combinations of their peer's capabilities" and that omitted = unsupported. It does **not** say the agent refuses to start. The expected behavior is **graceful degradation**: the agent will not send `fs/*` or `terminal/*` requests to a client that didn't advertise them. For Claude Code (via the adapter), which heavily uses file edits and terminal execution, advertising `fs=false`/`terminal=false` would force it into a degraded path — it would have to surface edits/commands through `session/request_permission` or text diffs rather than direct application. Whether the adapter *refuses* to start in that state is an adapter-specific behavior, not spec-mandated; Ora should treat it as "degraded, not refused" but verify empirically against `claude-agent-acp`.

### Recommendation for Ora

As an IDE that owns the editor + a terminal pane, Ora **should** advertise `fs.read_text_file=true`, `fs.write_text_file=true`, `terminal=true` in its `initialize` request — otherwise it cripples the very agents it's trying to host. The B2 design's use of `ClientCapabilities::default()` is a **gap to amend**: build the capabilities up with `.fs(FileSystemCapabilities::new().read_text_file(true).write_text_file(true)).terminal(true)` before sending `initialize`.

---

## [3] Baseline vs opt-in method surface

### Spec text

`docs/protocol/v1/initialization.mdx` ("Session Capabilities" subsection):

> As a baseline, all Agents **MUST** support `session/new`, `session/prompt`, `session/cancel`, and `session/update`.
>
> Optionally, they **MAY** support other session methods and notifications by specifying additional capabilities.

`initialize` itself is mandatory (the handshake: "Clients **MUST** initialize the connection by calling the `initialize` method"). Prompt content baseline: "As a baseline, all Agents **MUST** support `ContentBlock::Text` and `ContentBlock::ResourceLink` in `session/prompt` requests." (`initialization.mdx`, Prompt capabilities).

Opt-in methods and their gating capabilities:

| Method | Gating capability |
|--------|-------------------|
| `session/load` | `agentCapabilities.loadSession` (top-level, not under `sessionCapabilities`) |
| `session/resume` | `agentCapabilities.sessionCapabilities.resume` |
| `session/close` | `agentCapabilities.sessionCapabilities.close` |
| `session/list` | `agentCapabilities.sessionCapabilities.list` |
| `session/delete` | `agentCapabilities.sessionCapabilities.delete` |
| `session/set_mode` | advertised via modes in `session/new` response (no separate cap) |
| `session/set_config_option` | client-side `session.configOptions.boolean` cap |
| `fs/read_text_file`, `fs/write_text_file` | client-side `fs.*` caps |
| `terminal/*` (create/output/release/wait_for_exit/kill) | client-side `terminal` cap |
| `logout` | `agentCapabilities.auth.logout` |
| `authenticate` | advertised via `authMethods[]` in initialize response |

### Cross-check against Ora's DTOs

`crates/contracts/src/acp/literals.rs`:
- `AGENT_METHOD_NAMES` (lines 39–54): `initialize`, `authenticate`, `cancel_request` (`$/cancel_request`), `session_new`, `session_load`, `session_set_mode`, `session_set_config_option`, `session_prompt`, `session_cancel`, `session_list`, `session_delete`, `session_resume`, `session_close`, `logout`. Matches the spec method surface.
- `crates/contracts/src/acp/initialization.rs` `SessionCapabilities` (lines 497–547): `list`, `delete`, `additional_directories`, `resume`, `close` — all `Option<EmptyObject>`. The doc-comment at line 487 restates the baseline verbatim ("all Agents MUST support `session/new`, `session/prompt`, `session/cancel`, and `session/update`"). `load_session` lives on `AgentCapabilities` (line 319) with the same "will be unified in future versions" note the spec carries.

**Confirmed: Ora's baseline/opt-in split matches the spec exactly.** The B2 design's claim (baseline = new/prompt/cancel/update; everything else opt-in) is correct.

---

## [4] `session/load` replay semantics

### Spec text (v1)

`docs/protocol/v1/session-setup.mdx` ("Loading Sessions"):

> The Agent **MUST** replay the entire conversation to the Client in the form of `session/update` notifications (like `session/prompt`).
>
> … user_message_chunk and agent_message_chunk examples …
>
> When **all** the conversation entries have been streamed to the Client, the Agent **MUST** respond to the original `session/load` request.

The response body example is:

```json
{ "jsonrpc": "2.0", "id": 1, "result": null }
```

So in v1, **the `session/load` response body carries NO history**. Replay happens entirely through `session/update` notifications streamed *before* the response. The response is effectively an ack (`null`).

### Ora's DTO (confirmation + minor divergence)

`crates/contracts/src/acp/session.rs` `LoadSessionResponse` (lines 200–209):

```rust
pub struct LoadSessionResponse {
    pub modes: Option<SessionModeState>,
    pub config_options: Option<Vec<SessionConfigOption>>,
}
```

Ora's `LoadSessionResponse` carries `modes`/`config_options` but **no message history** — consistent with the spec's "history via notifications, not response body." Ora's response is a *superset* of the spec's `null` (agents that only return `null` still deserialize fine because both fields are `#[serde(default)]`). This is a benign, spec-compatible divergence. The design doc's claim that "LoadSessionResponse carries only modes/config_options" is accurate, and its concern about "replay semantics being unclear" is **resolved**: the spec unambiguously specifies replay-via-`session/update`-before-response.

### v2 removes `session/load`

`docs/protocol/v2/session-setup.mdx` shows **only `session/new` and `session/resume`** — `session/load` is gone. In v2, history replay is requested via a `replayFrom` cursor on `session/resume`:

> By default, resume restores the session context without replaying prior conversation history. Clients that need history replay can request it with `replayFrom`.
>
> To request full history replay, Clients set `replayFrom` to `{ "type": "start" }`:
> When `replayFrom.type` is `"start"`, the Agent **MUST** replay the entire conversation to the Client in the form of `session/update` notifications before responding.

v2 replay also uses **non-chunk upsert variants** (`user_message`, `agent_message`, `agent_thought` — full `content` arrays, upserts keyed by `messageId`) in addition to chunks.

### Ora v1/v2 gap

Ora's DTOs are **v1-shaped**:
- `ResumeSessionRequest` (session.rs lines 247–265) has **no `replay_from` field** — v1 resume never replays.
- `SessionUpdate` enum (lines 557–580) has only `UserMessageChunk`/`AgentMessageChunk`/`AgentThoughtChunk` — **no non-chunk `UserMessage`/`AgentMessage`/`AgentThought` upsert variants** that v2 replay uses.
- `LoadSessionRequest`/`LoadSessionResponse` are present (v1).

This is fine if Ora targets **protocol version 1** (which the adapters advertise — see Q6). Migrating to v2 later requires: adding `replayFrom` to `ResumeSessionRequest`, adding non-chunk upsert `SessionUpdate` variants, and dropping `session/load`. The B2 design's deferral of `session/load` to v2 is actually moot for v2 (it's removed), but the *intent* — don't build load/replay in MVP — is sound.

---

## [5] `session/close` vs `session/ended`

### Spec text

`docs/protocol/v1/session-setup.mdx` ("Closing Active Sessions") and the v2 counterpart both define exactly one terminal session method:

> ## Closing Active Sessions
> Agents that advertise `sessionCapabilities.close` allow Clients to tell the Agent to cancel any ongoing work for a session and free any resources associated with that active session.
>
> To close an active session, Clients **MUST** call the `session/close` method with the session ID.
> The Agent **MUST** cancel any ongoing work for that session as if `session/cancel` had been called, then free the resources associated with the session.

There is **no `session/ended` method** anywhere in the v1 or v2 spec. `session/close` is opt-in via `sessionCapabilities.close`. (There is also a `session/cancel` *notification* — baseline — which cancels an in-flight prompt but does not close the session.)

### Cross-check against Ora

- `crates/contracts/src/acp/literals.rs`: method names include `session_close` → `"session/close"` (line 81) and `session_cancel` → `"session/cancel"` (line 73). **No `session/ended` string anywhere.**
- `crates/contracts/src/acp/session.rs` `SessionUpdate` enum (lines 557–580): variants are `UserMessageChunk`, `AgentMessageChunk`, `AgentThoughtChunk`, `ToolCall`, `ToolCallUpdate`, `Plan`, `AvailableCommandsUpdate`, `CurrentModeUpdate`, `ConfigOptionUpdate`, `SessionInfoUpdate`, `UsageUpdate`. **No `Ended` variant.**
- `CloseSessionRequest`/`CloseSessionResponse` (session.rs lines 348–374) match the spec's `{sessionId}` request / `{}` response.

**Confirmed: the B2 design's concern about a `session/ended` method is unfounded — it does not exist.** Ora should use `session/close` (when `sessionCapabilities.close` is advertised) to terminate a session, and treat `session/cancel` as the in-flight-prompt cancel. No amendments needed to Ora's DTOs here.

---

## [6] Real agent launch + protocol version

### Protocol version

- Spec README (`agentclientprotocol/agent-client-protocol` root `README.md`): "**The current stable ACP protocol version is `1`.**" Wire compatibility is negotiated via `protocolVersion` in `initialize`.
- The Claude adapter confirms it sends **`protocolVersion: 1`** in its `initialize` response (`zed-industries/claude-agent-acp` `src/acp-agent.ts` line ~1330, inside `async initialize(...)`).

**Ora should send `protocolVersion: 1` in `InitializeRequest`** and expect the agent to echo `1` (or respond with its latest, which Ora must then accept or disconnect per the spec's version-negotiation rule).

### Claude Code — via adapter, NOT a `claude --acp` flag

The ACP agents registry (`docs/get-started/agents.mdx`) explicitly notes:

> - [Claude Agent](https://platform.claude.com/docs/en/agent-sdk/overview) (via [Zed's SDK adapter](https://github.com/zed-industries/claude-agent-acp))

So Claude Code does **not** expose ACP natively. The official path is the `@agentclientprotocol/claude-agent-acp` npm package (originally `zed-industries/claude-agent-acp`; repo homepage points to `agentclientprotocol/claude-agent-acp`).

- `package.json`:
  ```json
  { "name": "@agentclientprotocol/claude-agent-acp",
    "bin": { "claude-agent-acp": "dist/index.js" },
    "type": "module",
    "author": "Zed Industries" }
  ```
- `src/index.ts`: `#!/usr/bin/env node`; on plain launch (no `--cli`/`--version`) it calls `runAcp()` from `./acp-agent.ts`, redirects all console methods to stderr, and keeps stdin open. It wraps the `@anthropic-ai/claude-agent-sdk`. A `--cli` passthrough mode forwards args to the native `claude` CLI (so the adapter also sub-wraps the real CLI binary).
- `src/acp-agent.ts` (334 KB) registers `methods.agent.initialize` → `agent.initialize` (line ~7224), returns `protocolVersion: 1` (line ~1330) and `agentInfo`.

**Launch command for Ora's `Bun.spawn`:** `claude-agent-acp` (resolved from `node_modules/.bin` after `npm i @agentclientprotocol/claude-agent-acp`, or `npx @agentclientprotocol/claude-agent-acp`). No `--acp`/`--agent-client-protocol` flag on `claude` itself exists. Ora must launch the adapter as a stdio subprocess.

### Codex — via adapter, NOT a `codex --acp` flag

Registry:

> - [Codex CLI](https://developers.openai.com/codex/cli) (via [Zed's adapter](https://github.com/zed-industries/codex-acp))

`zed-industries/codex-acp` `README.md`:

> Development is moving to [agentclientprotocol/codex-acp](https://github.com/agentclientprotocol/codex-acp). The new adapter is built on the new Codex App Server… Use `@agentclientprotocol/codex-acp` for new installs.
>
> Install the adapter from the latest release… You can then use `codex-acp` as a regular ACP agent:
> ```
> OPENAI_API_KEY=sk-... codex-acp
> ```
> Or via npm:
> ```
> npx @zed-industries/codex-acp
> ```

The legacy `zed-industries/codex-acp` is Rust (`Cargo.toml` at root). The new `agentclientprotocol/codex-acp` is built on the Codex App Server. Either way, the launchable thing is a `codex-acp` stdio subprocess, not `codex --acp`.

**Launch for Ora:** `codex-acp` (with `OPENAI_API_KEY` or `CODEX_API_KEY` env, or ChatGPT-subscription auth). Prefer `@agentclientprotocol/codex-acp` for new installs.

### OpenCode — registry-listed as native, launch command UNVERIFIED

Registry:

> - [OpenCode](https://github.com/sst/opencode)

No "(via adapter)" note, unlike Claude/Codex — implying OpenCode exposes ACP natively. However, **I could not locate the ACP stdio server code or an exact launch flag** within `sst/opencode` from primary sources in this pass: `packages/protocol/src` contains `api.ts`/`errors.ts`/`groups/`/`middleware/` (appears to be opencode's own HTTP API, not ACP), and `packages/server/src` contains HTTP server code (no ACP handler found). The repo's `README.md` does not mention ACP/agent-client-protocol.

**UNVERIFIED — `<reason>`:** OpenCode's ACP entrypoint is not discoverable from the repo tree / README in this pass. Ora should either (a) consult opencode's official docs directly for the ACP launch command, or (b) test `opencode acp` / `opencode --acp` empirically before assuming a launch form. Do not assume OpenCode mirrors the Claude/Codex adapter pattern — it may have a built-in subcommand instead.

### Implication for the B2 design

The B2 spec's `Bun.spawn` tool must launch **adapter binaries** (`claude-agent-acp`, `codex-acp`) — not `claude`/`codex` with a hypothetical ACP flag. Ora's plugin-sdk should resolve the agent binary by configured name (e.g. from a registry of installed adapters) and spawn it with stdio pipes, then run the ACP `initialize` handshake over newline-delimited JSON. The "agent = a local CLI with an `--acp` flag" mental model should be replaced with "agent = an ACP server subprocess (often an adapter wrapping a non-ACP agent)".

---

## Recommendation for the Ora B2 design

1. **Framing: keep "newline-delimited JSON-RPC over stdio."** Verified against spec (`docs/protocol/v1/transports.mdx`), the reference Rust runtime (`rust-sdk` `stdio.rs` + `jsonrpc/transport_actor.rs`), and the Claude adapter. Serialize with **compact JSON** (no embedded newlines); read stdout line-by-line; never parse stderr as ACP. **No change to the B2 doc's framing choice** — remove the UNVERIFIED flag.

2. **Amend `ClientCapabilities` advertisement.** The B2 design currently leans on `ClientCapabilities::default()` (fs=false, terminal=false). Ora is an IDE that *can* read/write files and run terminals — it **should** advertise:
   ```rust
   ClientCapabilities::new()
       .fs(FileSystemCapabilities::new().read_text_file(true).write_text_file(true))
       .terminal(true)
   ```
   Otherwise coding agents degrade (can't apply edits or run commands directly). All client capabilities are spec-OPTIONAL, so Ora won't *break* handshake with them false, but the agent experience will be crippled. (All combinations are spec-valid; agents SHOULD support any combination.)

3. **Baseline method assumptions: confirmed correct.** Baseline = `initialize`, `session/new`, `session/prompt`, `session/cancel`, `session/update`. No amendment. Ora's `literals.rs` + `initialization.rs` already encode this.

4. **`session/load`:** The spec specifies replay unambiguously (history via `session/update` notifications *before* the response; response body `null` or modes/config_options). Ora's `LoadSessionResponse` (modes/config_options, no history) is a spec-compatible superset. **No amendment needed for v1.** Note for roadmap: v2 *removes* `session/load` and folds replay into `session/resume` via a `replayFrom` cursor — if/when Ora moves to v2, it must add `replayFrom` to `ResumeSessionRequest` and add non-chunk `UserMessage`/`AgentMessage`/`AgentThought` upsert variants to `SessionUpdate`. Defer is fine for MVP.

5. **`session/close` vs `session/ended`:** No `session/ended` exists in spec or Ora DTOs. Use `session/close` (opt-in via `sessionCapabilities.close`) to terminate; `session/cancel` (baseline notification) to abort an in-flight prompt. **No amendment.**

6. **Agent launch: amend the spawn model.** Launch adapter binaries, not bare CLIs:
   - Claude Code → `claude-agent-acp` (`@agentclientprotocol/claude-agent-acp`)
   - Codex → `codex-acp` (`@agentclientprotocol/codex-acp` preferred, `@zed-industries/codex-acp` legacy)
   - OpenCode → UNVERIFIED; consult opencode docs or test `opencode acp` empirically before assuming a form.
   - Send `protocolVersion: 1` in `initialize`.

7. **`StopReason` (bonus, from `prompt.rs`):** 5 variants — `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled`. Matches spec. `session/cancel` MUST return `cancelled` even if underlying ops throw. No amendment.

8. **v1 vs v2 awareness for the roadmap:** Ora's DTOs are v1-shaped (chunk-only `SessionUpdate`, `session/load` present, `ResumeSessionRequest` without `replayFrom`, `SessionCapabilities` without `set_mode`/`set_config_option` gating). This is correct for v1 (which all surveyed real agents advertise). A v2 migration is a separate future effort — the `docs/protocol/v2/migration.mdx` (50 KB) is the authoritative v1→v2 delta.

---

## Sources

### Authoritative ACP spec + schema repo
- Repo: `agentclientprotocol/agent-client-protocol` (GitHub). Root `README.md` (protocol version = 1; schema crate vs runtime crate split). `gh api repos/agentclientprotocol/agent-client-protocol/contents/README.md`.
  - Note: the task hint said "zed-industries/agent-client-protocol" — the actual canonical org is `agentclientprotocol`. `zed-industries/claude-agent-acp` and `zed-industries/codex-acp` exist as legacy adapter homes.
- Spec site: `https://agentclientprotocol.com` — WebFetch was blocked on this machine ("Unable to verify if domain … is safe to fetch"). All spec content was instead read from the canonical `docs/protocol/{v1,v2}/*.mdx` files in the repo (same source the site renders).
- `docs/protocol/v1/transports.mdx` — stdio framing = newline-delimited. `gh api repos/agentclientprotocol/agent-client-protocol/contents/docs/protocol/v1/transports.mdx`.
- `docs/protocol/v2/transports.mdx` — + JSON-RPC batch. `gh api …/contents/docs/protocol/v2/transports.mdx`.
- `docs/protocol/v1/initialization.mdx` — capabilities OPTIONAL/SHOULD, baseline methods, client/agent cap fields. `gh api …/contents/docs/protocol/v1/initialization.mdx`.
- `docs/protocol/v1/session-setup.mdx` — `session/load` replay-via-`session/update` + `null` response; `session/close`; `session/resume`. `gh api …/contents/docs/protocol/v1/session-setup.mdx`.
- `docs/protocol/v2/session-setup.mdx` — `session/load` removed; `replayFrom` cursor on `session/resume`; non-chunk upsert variants. `gh api …/contents/docs/protocol/v2/session-setup.mdx`.
- `docs/get-started/agents.mdx` — agent registry (Claude/Codex via adapters; OpenCode native). `gh api …/contents/docs/get-started/agents.mdx`.
- `docs/get-started/clients.mdx` — client registry. `gh api …/contents/docs/get-started/clients.mdx`.

### Reference implementation (runtime crate)
- Repo: `agentclientprotocol/rust-sdk` (source of the higher-level `agent-client-protocol` runtime crate referenced by the schema repo README).
- `src/agent-client-protocol/src/stdio.rs` — `BufReader::lines()` read, `jsonrpc::write_line` write. `gh api repos/agentclientprotocol/rust-sdk/contents/src/agent-client-protocol/src/stdio.rs`.
- `src/agent-client-protocol/src/jsonrpc/transport_actor.rs` — `parse_incoming_line(line: &str)` per-line parse. `gh api …/contents/src/agent-client-protocol/src/jsonrpc/transport_actor.rs`.

### Real agent ACP servers
- `zed-industries/claude-agent-acp` (→ `agentclientprotocol/claude-agent-acp`):
  - `README.md`, `package.json` (bin `claude-agent-acp`, npm `@agentclientprotocol/claude-agent-acp`, author Zed Industries).
  - `src/index.ts` — stdout reserved for ACP, console→stderr, `runAcp()`.
  - `src/acp-agent.ts` — `initialize` handler returns `protocolVersion: 1` (~line 1330) and `agentInfo`; registers `methods.agent.initialize` (~line 7224).
- `zed-industries/codex-acp` (→ `agentclientprotocol/codex-acp`):
  - `README.md` — `codex-acp` binary; `OPENAI_API_KEY=sk-... codex-acp`; `npx @zed-industries/codex-acp`; migration to `@agentclientprotocol/codex-acp` on Codex App Server.
- `sst/opencode` — registry-listed as ACP-native; **ACP launch command UNVERIFIED** in this pass (no ACP code located in `packages/protocol/src` or `packages/server/src`; README does not mention ACP).

### Ora local primary sources
- `E:\claude_code_project\desktop\crates\contracts\src\acp\rpc.rs` — `JsonRpcMessage`, `RequestId`, `Response`, `Notification`, `JsonRpcBatch` (envelope only; no transport).
- `E:\claude_code_project\desktop\crates\contracts\src\acp\literals.rs` — `AGENT_METHOD_NAMES` / `CLIENT_METHOD_NAMES`; confirms no `session/ended`.
- `E:\claude_code_project\desktop\crates\contracts\src\acp\initialization.rs` — `ProtocolVersion(pub u16)`; `ClientCapabilities::default()` = fs=false/terminal=false; `SessionCapabilities` baseline comment (line 487); `load_session` on `AgentCapabilities`.
- `E:\claude_code_project\desktop\crates\contracts\src\acp\session.rs` — `LoadSessionResponse` (modes/config_options, no history, lines 200–209); `ResumeSessionRequest` (no `replayFrom`, v1); `CloseSessionRequest`/`Response`; `SessionUpdate` variants (no `Ended`).
- `E:\claude_code_project\desktop\crates\contracts\src\acp\notification.rs` — `SessionNotification`, `CancelNotification`, `CancelRequestNotification`.
- `E:\claude_code_project\desktop\crates\contracts\src\acp\prompt.rs` — `StopReason` (end_turn, max_tokens, max_turn_requests, refusal, cancelled).
- `E:\claude_code_project\desktop\packages\mock-service\src\acp.ts` — Ora's mock is a pure in-memory `AcpClient` (no stdio framing); `transport.ts` is HTTP/MSW. **The mock does NOT back the B2 framing choice** — the framing is backed solely by the spec + reference impl + adapters above.
