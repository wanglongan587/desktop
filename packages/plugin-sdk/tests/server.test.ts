import { describe, expect, it, vi } from "vitest";
import { methods } from "../src/protocol";
import type { JsonRpcInbound } from "../src/protocol";
import { servePlugin } from "../src/server";

/** Parses a binary frame Buffer into its JSON-RPC message. */
function parseFrameMessage(buf: Buffer): unknown {
  const length = buf.readInt32BE(1);
  const payload = buf.subarray(5, 5 + length).toString();
  return JSON.parse(payload);
}

describe("servePlugin", () => {
  it("dispatches requests to handlers, sends responses, and pushes notifications", async () => {
    const spy = vi.spyOn(process.stdout, "write").mockReturnValue(true);
    const messages: (JsonRpcInbound | null)[] = [
      { jsonrpc: "2.0", id: "1", method: methods.agentNewSession, params: {} },
      { jsonrpc: "2.0", id: "2", method: methods.agentPrompt, params: { sessionId: "s1" } },
      null,
    ];
    let readIndex = 0;
    const read = vi.fn(async () => messages[readIndex++] ?? null);

    await servePlugin(
      {
        [methods.agentNewSession]: async () => ({ sessionId: "s1" }),
        [methods.agentPrompt]: async (_params, notify) => {
          notify(methods.agentSessionUpdate, {
            sessionId: "s1",
            update: { kind: "chunk" },
          });
          return { stopReason: "endTurn" };
        },
      },
      { read },
    );

    const written = spy.mock.calls.map((call) => parseFrameMessage(call[0] as Buffer));
    expect(written).toEqual([
      { jsonrpc: "2.0", id: "1", result: { sessionId: "s1" } },
      {
        jsonrpc: "2.0",
        method: "agent/sessionUpdate",
        params: { sessionId: "s1", update: { kind: "chunk" } },
      },
      { jsonrpc: "2.0", id: "2", result: { stopReason: "endTurn" } },
    ]);
    spy.mockRestore();
  });

  it("reports method-not-found for unknown methods and ignores inbound notifications", async () => {
    const spy = vi.spyOn(process.stdout, "write").mockReturnValue(true);
    const messages: (JsonRpcInbound | null)[] = [
      { jsonrpc: "2.0", id: "1", method: "unknown", params: {} },
      { jsonrpc: "2.0", method: "agent/sessionUpdate", params: {} },
      null,
    ];
    let readIndex = 0;
    const read = vi.fn(async () => messages[readIndex++] ?? null);

    await servePlugin({}, { read });

    const written = spy.mock.calls.map((call) => parseFrameMessage(call[0] as Buffer));
    expect(written).toEqual([
      {
        jsonrpc: "2.0",
        id: "1",
        error: { code: -32601, message: "Method not found: unknown" },
      },
    ]);
    spy.mockRestore();
  });
});
