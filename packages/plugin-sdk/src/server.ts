import { readMessage } from "./internal/reader";
import { sendError, sendNotification, sendResponse } from "./internal/writer";
import type { JsonRpcInbound, JsonRpcRequest } from "./protocol";

/** Handler for one plugin-channel method. `notify` pushes a notification to the host. */
export type PluginMethodHandler = (
  params: unknown,
  notify: (method: string, params: unknown) => void,
) => unknown | Promise<unknown>;

/** Method-name → handler map. */
export interface PluginServerHandlers {
  [method: string]: PluginMethodHandler | undefined;
}

export interface ServePluginOptions {
  /** Inject a custom reader for tests; defaults to reading stdin. */
  read?: () => Promise<JsonRpcInbound | null>;
}

/** Returns true when the message is a request (has `id`), false for notifications. */
function isRequest(message: JsonRpcInbound): message is JsonRpcRequest {
  return "id" in message;
}

/**
 * Runs the plugin-channel read loop: reads each host message, dispatches requests to their
 * handler (pushing notifications through `notify`), and ignores inbound notifications.
 *
 * Returns when stdin closes. Handler errors are reported as JSON-RPC error responses;
 * unknown methods report `method not found`.
 */
export async function servePlugin(
  handlers: PluginServerHandlers,
  options: ServePluginOptions = {},
): Promise<void> {
  const read = options.read ?? readMessage;
  const notify = (method: string, params: unknown) => sendNotification(method, params);

  for (;;) {
    const message = await read();
    if (message === null) return;
    if (!isRequest(message)) continue;

    const handler = handlers[message.method];
    if (!handler) {
      sendError(message.id, -32601, `Method not found: ${message.method}`);
      continue;
    }
    try {
      const result = await handler(message.params, notify);
      sendResponse(message.id, result);
    } catch (error) {
      sendError(
        message.id,
        -32603,
        error instanceof Error ? error.message : "internal error",
      );
    }
  }
}
