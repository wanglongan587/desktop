// console protection must run before any other API is exposed — it redirects console
// output to stderr so stdout stays clean for the binary-frame plugin channel.
import "./console-guard";

export { readFrame, readMessage } from "./internal/reader";
export { FRAME_TYPE, methods } from "./protocol";
export { sendError, sendNotification, sendResponse, writeFrame, writeLine } from "./internal/writer";
export { servePlugin } from "./server";
export type {
  Frame,
  JsonRpcError,
  JsonRpcErrorResponse,
  JsonRpcInbound,
  JsonRpcNotification,
  JsonRpcRequest,
  JsonRpcSuccessResponse,
} from "./protocol";
export type {
  PluginMethodHandler,
  PluginServerHandlers,
  ServePluginOptions,
} from "./server";
