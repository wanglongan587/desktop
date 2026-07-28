import { methods, servePlugin } from "@ora-space/plugin-sdk";

/** The single canned session id this mock agent hands out and streams updates for. */
const SESSION_ID = "mock-session-1";

/**
 * Mock agent plugin: a canned reference that answers `initialize` / `agent/newSession` /
 * `agent/prompt` / `agent/cancel` / `shutdown` over the plugin channel, streaming two
 * `agent_message_chunk` updates before ending the turn. Validates the Ora plugin runtime
 * end-to-end without depending on a real agent binary.
 */
void servePlugin({
  [methods.initialize]: async () => ({ kind: "agent", version: "0.1.0" }),
  [methods.agentNewSession]: async () => ({ sessionId: SESSION_ID }),
  [methods.agentPrompt]: async (_params, notify) => {
    notify(methods.agentSessionUpdate, {
      sessionId: SESSION_ID,
      update: {
        sessionUpdate: "agent_message_chunk",
        content: { type: "text", text: "Hello from the mock agent." },
      },
    });
    notify(methods.agentSessionUpdate, {
      sessionId: SESSION_ID,
      update: {
        sessionUpdate: "agent_message_chunk",
        content: { type: "text", text: "This is a canned second chunk." },
      },
    });
    return { stopReason: "end_turn" };
  },
  [methods.agentCancel]: async () => undefined,
  [methods.shutdown]: async () => {
    process.exit(0);
  },
});
