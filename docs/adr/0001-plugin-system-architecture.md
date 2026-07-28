# Plugin system architecture: code-bearing out-of-process plugins with kind-first runtimes

The plugin system is built on **code-bearing plugins that run as host-managed child
processes** (Model B), speaking JSON-RPC 2.0 over stdio with the Host. A manifest
declares a `PluginKind`; the Host runs a per-kind `PluginRuntime`. For the
`agent` kind, the plugin owns the ACP conversation (spawns the Agent, bridges the
Host channel to ACP), and the Host's agent-runtime implements the existing
`AcpClient` by bridging to the plugin — so `packages/chat`'s `AcpClient` signature
stays unchanged and only its implementation is replaced.

## Considered Options

- **Data-only manifest, Host owns ACP (Model A)** — rejected: would make the
  plugin a pure declaration and the Host the ACP client. Simpler for codex, but
  the user chose code-bearing plugins so non-ACP agents and custom logic can live
  in the plugin. Reversing later would mean reshaping the SDK.
- **In-process loading (WASM / native module)** — rejected: WASM cannot easily
  spawn child processes (needs WASI + Host capability injection), native modules
  give no isolation/safety, and both constrain plugin language. Out-of-process
  matches the existing `plugin-sdk` stdio model and ACP's own child-process model.
- **Capability-only dispatch (no `kind`)** — rejected: agent session lifecycle
  and UI render lifecycle differ too much to flatten into one capability bag;
  an enum `kind` + per-kind runtime matches the project's "type" mental model and
  `AGENTS.md`'s enum-over-bool-bag rule.
- **ACP tunneled directly over the plugin channel** — rejected: would lock the
  Host↔plugin contract to ACP's wire format, so ACP evolution or a non-ACP agent
  would freeze the plugin contract. The agent-kind contract is a stable
  agent-shaped method set instead.

## Consequences

- Every plugin is a process; agent dialogue has one extra hop
  (`Host → plugin → Agent`). Accepted for isolation + language-agnosticism.
- The `plugin-sdk` (currently a `getNums`/`returnNums` toy with request/response
  only) must grow **notifications** and **streaming** so an agent plugin can push
  `session/update` asynchronously.
- `createUnavailableAcpClient` is replaced by a plugin-bridge `AcpClient`
  implementation in the agent-runtime.
- Adding a plugin type = extend `PluginKind` + add a `PluginRuntime` impl; no
  change to the chat domain or the installable-manifest shape.
