# Ora Plugin System

The domain of extending Ora with installable, code-bearing plugins. A plugin runs
as a host-managed process and is driven by the Host through a defined lifecycle.
This context exists to unify agent, UI, and workbench extensions under one model
so new plugin types can be added without reshaping the system.

## Language

**Host**:
The Ora process that discovers, installs, enables, and drives Plugins.
_Avoid_: app, main, server

**Plugin**:
An installable, code-bearing extension of Ora, managed through a defined lifecycle
and driven by the Host over a Plugin Channel.
_Avoid_: extension, add-on, module

**Plugin Process**:
The OS child process a Plugin runs as, spawned by the Host from the Plugin
Manifest's process entrypoint. Distinct from any Agent process the plugin itself
spawns internally.
_Avoid_: plugin instance, worker

**Agent Plugin**:
A Plugin that bridges the Host to a specific Agent over ACP — it owns the spawn
of the Agent process and the ACP conversation with it.
_Avoid_: agent adapter, agent wrapper, connector

**Agent**:
An external process implementing the ACP server that an Agent Plugin connects to
(e.g. codex). Distinct from the Agent Plugin that bridges to it.
_Avoid_: model, assistant, llm

**ACP**:
Agent Client Protocol; the client/server protocol spoken between an Agent Plugin
(client) and an Agent (server). Carries initialize, session/new, session/prompt,
session/cancel, and the session/update notification stream.
_Avoid_: — (canonical protocol name)

**Plugin Channel**:
The message link between the Host and a running Plugin. A message is carried in a binary frame.
_Avoid_: transport, wire, pipe

**Frame**:
One binary unit on the Plugin Channel: `[type: i8][length: i32 big-endian][payload: n bytes]`.
The 5-byte header carries a `type` byte (payload content selector) and a `length`
(payload byte count, not counting the header). Total frame size = 5 + length.
_Avoid_: packet, message

**Frame Type**:
The `i8` byte at the start of a Frame header. Selects how the payload is interpreted.
Currently `1` = JSON (JSON-RPC 2.0 text). Reserved: `2` = file/binary.
Not fixed—a new type does not change the Frame structure.
_Avoid_: payload kind, encoding

**Plugin SDK**:
The library a Plugin author writes against; provides the Plugin Channel primitives
and the contract surface for each plugin kind.
_Avoid_: host sdk, runtime

**Plugin Manifest**:
The cross-kind descriptor of an installable Plugin — its id, version, kind,
process entrypoint, capabilities, and display metadata. The Host-interpreted
fields (the process entrypoint) describe how to spawn the Plugin Process; any
kind-specific payload is Plugin-interpreted config that the Host passes through.
_Avoid_: plugin config, descriptor

**Agent Definition**:
The live, chat-referenceable description of an agent, derived from an Agent
Plugin's Manifest when the plugin is activated. The chat domain references agents
by it; it carries no install-time data.
_Avoid_: agent config, plugin entry

## States

**Enabled**:
A user opt-in state: an Installed Plugin the user has turned on, eligible to be
activated on demand.
_Avoid_: active, running

**Started**:
A Plugin Process has been spawned and the Plugin Channel handshake (kind /
capability negotiation) has completed; the kind runtime is not yet initialized.
_Avoid_: activated

**Activated**:
The kind runtime is initialized and ready to execute. For an Agent Plugin this
means ACP initialize has completed with the Agent and a session can be opened.
_Avoid_: started, enabled
