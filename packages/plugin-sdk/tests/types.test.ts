import assert from "node:assert/strict";
import test from "node:test";

import type {
  ActivateReason,
  AgentEvent,
  CancelRequestParams,
  DeactivateReason,
  DeactivateResult,
  InitializeParams,
  JsonObject,
  JsonRpcError,
  JsonRpcRequest,
  JsonRpcSuccessResponse,
  JsonRpcVersion,
  JsonValue,
  RequestId,
  StreamNotificationParams,
} from "../src/index.js";

// The v1 protocol surface (design-v3 §12.5, §12.8) is the sole source of truth; the legacy
// `add`/NDJSON DTOs are intentionally absent (§22.4). These assertions pin the wire-envelope and
// lifecycle shapes the Rust contract generates via ts-rs.

test("wire envelope DTOs project to the §12.5 shapes", () => {
  const request = {
    jsonrpc: "2.0",
    id: "h:1",
    method: "$/initialize",
    params: { wireVersion: 1 },
  } satisfies JsonRpcRequest;
  assert.equal(
    JSON.stringify(request),
    '{"jsonrpc":"2.0","id":"h:1","method":"$/initialize","params":{"wireVersion":1}}',
  );

  const success = {
    jsonrpc: "2.0",
    id: "h:1",
    result: { providers: [] },
  } satisfies JsonRpcSuccessResponse;
  assert.equal(success.id, "h:1");

  const error = { code: -32601, message: "method not found" } satisfies JsonRpcError;
  assert.equal(error.code, -32601);
});

test("lifecycle and control DTOs satisfy their §12.8 contracts", () => {
  const version: JsonRpcVersion = "2.0";
  assert.equal(version, "2.0");

  const id: RequestId = "h:1";
  assert.equal(id, "h:1");

  const activateReason: ActivateReason = "lazyInvocation";
  assert.equal(activateReason, "lazyInvocation");

  const deactivateReason: DeactivateReason = "grantChanged";
  assert.equal(deactivateReason, "grantChanged");

  const deactivateResult: DeactivateResult = null;
  assert.equal(deactivateResult, null);

  const stream: StreamNotificationParams = {
    id: "h:1",
    seq: 1,
    value: { kind: "textDelta", channel: "assistant", text: "hi" },
  };
  assert.equal(stream.seq, 1);

  const cancel: CancelRequestParams = { id: "h:1" };
  assert.equal(cancel.id, "h:1");
});

test("initialize params satisfy the §12.8 envelope and AgentEvent is the stream value union", () => {
  const event: AgentEvent = { kind: "textDelta", channel: "assistant", text: "hi" };
  const init: InitializeParams = {
    wireVersion: 1,
    hostVersion: "0.1.0",
    runtimeVersion: "0.1.0",
    sessionId: "sess-1",
    plugin: {
      id: "ora.claude-code",
      version: "0.1.0",
      kind: "agent",
      pluginApi: 1,
      contentOwner: "sha256-" + "a".repeat(64),
    },
    paths: {
      extensionPath: "D:/plugins/ora.claude-code",
      entryPath: "D:/plugins/ora.claude-code/dist/index.js",
      storagePath: "D:/plugin-data/ora.claude-code/sha256-owner",
    },
    declaredAgents: [{ id: "claude-code", contractVersion: 1 }],
    limits: {
      maxFrameBytes: 8388608,
      maxPendingRequests: 128,
      maxAgentEventBytes: 262144,
      maxAgentResultBytes: 1048576,
      maxAgentPromptBytes: 1048576,
      maxActiveTurns: 64,
      maxPageItems: 100,
    },
  };
  assert.equal(init.wireVersion, 1);
  assert.equal(init.plugin.kind, "agent");
  assert.equal(event.kind, "textDelta");
});

test("JsonValue and JsonObject accept JSON primitives", () => {
  const num: JsonValue = 42;
  const str: JsonValue = "hi";
  const bool: JsonValue = true;
  const nl: JsonValue = null;
  const obj: JsonObject = { kind: "agentUnavailable", retryable: false };
  assert.equal(num, 42);
  assert.equal(str, "hi");
  assert.equal(bool, true);
  assert.equal(nl, null);
  assert.equal(obj.kind, "agentUnavailable");
});
