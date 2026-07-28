import type {
  JsonRpcErrorResponse,
  JsonRpcNotification,
  JsonRpcSuccessResponse,
} from "../protocol";

const FRAME_TYPE_JSON = 1;

/// Writes a binary frame: `[type: i8][length: i32 big-endian][payload: n bytes]`.
/// Uses manual byte writing (no Rust struct) to avoid alignment padding.
export function writeFrame(frameType: number, payload: Buffer): void {
  const header = Buffer.alloc(5);
  header.writeInt8(frameType, 0);
  header.writeInt32BE(payload.length, 1);
  process.stdout.write(Buffer.concat([header, payload]));
}

/// Writes a type=1 (JSON-RPC) frame. Convenience wrapper around writeFrame.
export function writeLine(message: unknown): void {
  writeFrame(FRAME_TYPE_JSON, Buffer.from(JSON.stringify(message)));
}

/// Sends a successful response to a host request.
export function sendResponse(id: string | number | null, result: unknown): void {
  const response: JsonRpcSuccessResponse = { jsonrpc: "2.0", id, result: result ?? null };
  writeLine(response);
}

/// Sends an error response to a host request.
export function sendError(
  id: string | number | null,
  code: number,
  message: string,
): void {
  const response: JsonRpcErrorResponse = {
    jsonrpc: "2.0",
    id,
    error: { code, message },
  };
  writeLine(response);
}

/// Sends a notification (no id) to the host, e.g. an `agent/sessionUpdate`.
export function sendNotification(method: string, params: unknown): void {
  const notification: JsonRpcNotification = { jsonrpc: "2.0", method, params };
  writeLine(notification);
}
