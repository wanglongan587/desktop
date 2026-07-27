# Permission handling is confined to the ACP bridge; plugin-manager stays ACP-blind

Status: accepted

Ora agents (Claude Code, codex, opencode) gate their own tool calls via the ACP `session/request_permission` client method — they perform the file/terminal I/O themselves and only ask the client (the plugin process) to approve. We decided that this approval is handled entirely inside the plugin-sdk ACP bridge (`createAcpAgentProvider`), never in the Rust plugin-manager. The plugin-manager is and remains ACP-blind: it relays only ora-plugin-protocol `AgentEvent`. v1's bridge default is auto-Allow (AllowOnce + log); real frontend-approval — which requires a plugin→host→frontend request-response channel that ora-plugin-protocol v1 does not have — is v2.

## Consequences

- **plugin-manager is zero-change for permission policy.** Auto-Allow vs frontend-approval is a bridge-internal decision; the host never sees `session/request_permission`.
- **Tool-call visibility is decoupled from permission policy.** `AgentEvent` already carries `ToolCall` / `ToolResult` (`crates/plugin-protocol/src/agent/dto.rs:357`), so the UI sees every tool call (name, summary, success/error) in real time regardless of whether the bridge auto-allows or asks the frontend. "Auto-Allow" is not "invisible."
- **The one future coupling to plugin-manager** is optional, not permission-related: `AgentEvent::ToolCall` is sparse (call_id / name / summary) and does not carry ACP's rich tool-call data (Diff, file locations, ToolKind, status lifecycle). Surfacing those would require extending the `AgentEvent` DTO — a plugin-protocol change that touches plugin-manager. Defer until diff/follow-along UI is wanted.
- **Errata §14 of `docs/superpowers/specs/2026-07-21-b2-design-errata-and-amendments.md` is over-scoped.** Its "advertise fs/terminal=true + implement fs/* handlers" rests on the false premise that agents delegate file I/O to the client; claude-agent-acp and codex-acp do their own I/O and never send `fs/*` (those code paths are dead in the adapter). Only opencode uses `fs/write_text_file` (write-only, guarded). Revise §14 to: advertise `fs=false`, implement no fs handlers, and handle `session/request_permission` (auto-Allow v1).

## Considered Options

- **(Chosen) Bridge-internal handling, auto-Allow in v1.** No protocol change; plugin-manager untouched; ships fastest. Auto-Allow is the Claude Code skip-permissions equivalent — defensible for "own agent on own machine."
- **(Rejected for v1) Frontend-approval in v1.** Matches the product ideal (user gates the agent) but the agent blocks on `session/request_permission`, forcing a plugin→host→frontend request-response channel that breaks ora-plugin-protocol invariant 24 and requires a pluginApi upgrade. Pulled to v2 as the first channel deliverable.
