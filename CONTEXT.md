# Ora

Ora is an AI-agent IDE: a web frontend backed by a Rust host that runs per-plugin TypeScript (Bun) processes. Each agent plugin wraps a local agent runtime (Claude Code, codex, opencode) and speaks the Agent Client Protocol (ACP) to it.

## Language

**PluginKind**:
A plugin's kind, declared in its manifest as a tagged union: `Agent` or `Workbench`. The kind decides which path the plugin takes — only `Agent` has a runtime and a capability surface; `Workbench` is catalog-only in v1 (no process, no capabilities). The capability surface never crosses kinds.
_Avoid_: "plugin type" (collides with module/package type)

**Agent plugin**:
A plugin of kind `Agent`: it has an entry point, spawns a Bun process, and exposes the `AgentRequest` capability surface (8 methods, incl. `startConversation`). "Agent" here is the *kind*, not "speaks ACP" — an agent plugin may wrap a local ACP-speaking runtime via the optional `createAcpAgentProvider` scaffold, or hand-write an `AgentProvider` against any backend.
_Avoid_: "agent" (collides with the agent runtime itself); "a plugin that speaks ACP" (ACP is optional, not definitional)

**Workbench plugin**:
A plugin of kind `Workbench`: catalog-only in v1 (a `WorkbenchContribution{schemaVersion}` marker, no entry point, no process, no capability surface). A future v2 would give it its own `WorkbenchRequest` surface — never the agent's `startConversation`.
_Avoid_: "non-agent plugin" (too negative); conflating with agent plugins' surface

**Lifecycle vs capability (ora-plugin-protocol)**:
Two orthogonal request families on the ②↔③ wire. *Lifecycle* = `initialize` (handshake, kind==Agent check) → `activate` (reason `lazyInvocation|manualStart`, returns declared providers) → `deactivate`. *Capability* = `AgentRequest` (8 `agent.*` methods incl. `startConversation`). "Starting a plugin" = lifecycle; "starting a conversation" = a capability call that happens later, only when the user submits a prompt. `lazyInvocation` can run the two back-to-back on first prompt, but they remain distinct protocol messages with distinct timeouts.
_Avoid_: "starting the plugin" as if it meant `startConversation`

**DeclaredAgent vs AgentProvider**:
Two faces of one provider. `DeclaredAgent{id, contractVersion}` is the *metadata* the host (②) tracks. `AgentProvider` is the *live object* (8 methods) that exists only inside the plugin TS process (③). The host never holds the live object; it routes capability calls to it via the runtime.
_Avoid_: "provider" used ambiguously for both

**AgentRequest**:
The typed capability-call union (②→③) for agent plugins: `discoverInstallations / getConfigurationSummary / listSkills / listMcpServers / listConversations / startConversation / sendMessage / cancelConversation`. Pure capability surface — orthogonal to the lifecycle family.
_Avoid_: "agent request" as a verb

**createAcpAgentProvider (optional scaffold)**:
A helper in `plugin-sdk/acp` that builds an `AgentProvider` whose conversation methods drive a local ACP-speaking runtime (Claude Code, codex). Optional — only for agent plugins that wrap a local ACP CLI; an agent plugin may instead hand-write its `AgentProvider`. Not the definition of an agent plugin.
_Avoid_: "the ACP bridge" as if it were the agent-plugin mechanism itself

**AgentEvent**:
An ora-plugin-protocol event streamed from the plugin process to the Ora host during a turn — conversation started, text delta, status, tool call, tool result, usage.
_Avoid_: "stream event", "agent event" (collides with SessionUpdate)

**SessionUpdate**:
An ACP notification from the agent runtime to the plugin process during a turn. The plugin-sdk bridge translates these into AgentEvents.
_Avoid_: "agent event" (collides with AgentEvent)

**session/request_permission**:
An ACP client method: the agent asks the plugin process (as ACP client) to approve a tool call the agent will itself execute. Approval gating, not delegation.
_Avoid_: "permission" (too vague)

**fs / terminal (ClientCapabilities)**:
ACP delegation switches the plugin (as ACP client) advertises to the agent during `initialize`: `fs.{readTextFile,writeTextFile}` and `terminal`. When true, the agent asks the *plugin* to perform file/terminal I/O on its behalf. The ACP bridge advertises `false` for both, so the agent does its own I/O. This is *delegation*, distinct from `session/request_permission` which is *gating*.
_Avoid_: "file access", "terminal access"

**ACP-blind**:
The plugin-manager (Ora host) never speaks or sees ACP; it relays only ora-plugin-protocol. A B2-design invariant.
