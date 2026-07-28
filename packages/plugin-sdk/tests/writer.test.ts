import { describe, expect, it, vi } from "vitest";
import { sendError, sendNotification, sendResponse } from "../src/internal/writer";

/** Parses a binary frame Buffer into { type, message }. */
function parseFrame(buf: Buffer): { type: number; message: unknown } {
  const type = buf[0];
  const length = buf.readInt32BE(1);
  const payload = buf.subarray(5, 5 + length).toString();
  return { type, message: JSON.parse(payload) };
}

describe("writer", () => {
  it("writes a type=1 binary frame for a JSON-RPC success response", () => {
    const spy = vi.spyOn(process.stdout, "write").mockReturnValue(true);
    sendResponse("1", { ok: true });
    const frame = parseFrame(spy.mock.calls[0]?.[0] as Buffer);
    expect(frame.type).toBe(1);
    expect(frame.message).toEqual({
      jsonrpc: "2.0",
      id: "1",
      result: { ok: true },
    });
    spy.mockRestore();
  });

  it("writes a type=1 binary frame for a notification", () => {
    const spy = vi.spyOn(process.stdout, "write").mockReturnValue(true);
    sendNotification("agent/sessionUpdate", { sessionId: "s1" });
    const frame = parseFrame(spy.mock.calls[0]?.[0] as Buffer);
    expect(frame.type).toBe(1);
    expect(frame.message).toEqual({
      jsonrpc: "2.0",
      method: "agent/sessionUpdate",
      params: { sessionId: "s1" },
    });
    spy.mockRestore();
  });

  it("writes a type=1 binary frame for an error response", () => {
    const spy = vi.spyOn(process.stdout, "write").mockReturnValue(true);
    sendError("2", -32601, "Method not found: x");
    const frame = parseFrame(spy.mock.calls[0]?.[0] as Buffer);
    expect(frame.type).toBe(1);
    expect(frame.message).toEqual({
      jsonrpc: "2.0",
      id: "2",
      error: { code: -32601, message: "Method not found: x" },
    });
    spy.mockRestore();
  });
});
