/** Plugin-channel JSON-RPC 2.0 envelope types + binary frame constants. */

/**
 * Binary frame format: `[type: i8][length: i32 big-endian][payload: n bytes]`.
 * The 5-byte header carries a type byte (payload content selector) and a length
 * (payload byte count, not counting the header). Total frame size = 5 + length.
 */
export const FRAME_TYPE = {
  JSON: 1,
  // FILE: 2,  // reserved for future binary payload
} as const;

/** One binary frame read from the channel. */
export interface Frame {
  type: number;
  payload: Buffer;
}

/** Host → plugin request (carries an `id` the response must echo). */
export interface JsonRpcRequest<P = unknown> {
  jsonrpc: "2.0";
  id: string | number | null;
  method: string;
  params?: P;
}

/** Plugin → host successful response. */
export interface JsonRpcSuccessResponse<R = unknown> {
  jsonrpc: "2.0";
  id: string | number | null;
  result: R;
}

/** A JSON-RPC error object. */
export interface JsonRpcError {
  code: number;
  message: string;
  data?: unknown;
}

/** Plugin → host error response. */
export interface JsonRpcErrorResponse {
  jsonrpc: "2.0";
  id: string | number | null;
  error: JsonRpcError;
}

/** Plugin → host notification (no `id`; fire-and-forget, e.g. `agent/sessionUpdate`). */
export interface JsonRpcNotification<P = unknown> {
  jsonrpc: "2.0";
  method: string;
  params?: P;
}

/** A message read from the host: a request (has `id`) or a notification (no `id`). */
export type JsonRpcInbound<P = unknown> = JsonRpcRequest<P> | JsonRpcNotification<P>;

/**
 * Plugin-channel method names. These mirror `ora_contracts::plugin_methods`; the SDK keeps
 * its own copy because Rust string constants are not exported to TypeScript.
 */
export const methods = {
  initialize: "initialize",
  agentNewSession: "agent/newSession",
  agentPrompt: "agent/prompt",
  agentCancel: "agent/cancel",
  shutdown: "shutdown",
  agentSessionUpdate: "agent/sessionUpdate",
} as const;
