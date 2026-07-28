import { describe, expect, it, vi } from "vitest";
import type { acp, PromptSessionEvent } from "@ora/contracts";
import { createTauriPluginChatSessionClient } from "./tauri-chat-session-client";

type NotificationHandler = (event: { payload: acp.SessionNotification }) => void;

/** Flushes the microtask queue so the async generator drains buffered events. */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function sessionUpdate(sessionId: string, text: string): acp.SessionNotification {
  return {
    sessionId,
    update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text } },
  } as acp.SessionNotification;
}

describe("createTauriPluginChatSessionClient", () => {
  it("maps the Ora session to a plugin session, streams filtered updates, ends with the stop reason", async () => {
    let handler: NotificationHandler | null = null;
    const unlisten = vi.fn();
    const listenForUpdates = vi.fn(async (_event: string, h: NotificationHandler) => {
      handler = h;
      return unlisten;
    });
    let resolvePrompt: ((response: acp.PromptResponse) => void) | null = null;
    const invokeCommand = vi.fn(async (command: string) => {
      if (command === "plugin_agent_new_session") return { sessionId: "plugin-session-1" };
      if (command === "plugin_agent_prompt")
        return new Promise<acp.PromptResponse>((resolve) => {
          resolvePrompt = resolve;
        });
      if (command === "plugin_agent_cancel") return undefined;
      throw new Error(`unexpected invoke: ${command}`);
    });

    const client = createTauriPluginChatSessionClient({
      pluginId: "opencode",
      cwd: "/tmp",
      invokeCommand: invokeCommand as never,
      listenForUpdates: listenForUpdates as never,
    });

    const events: PromptSessionEvent[] = [];
    const done = (async () => {
      for await (const event of client.prompt({ sessionId: "ora-1", text: "hello" })) {
        events.push(event);
      }
    })();

    await vi.waitFor(() => expect(handler).not.toBeNull());
    const emit = handler!;

    // An update for a different plugin session must be filtered out.
    emit({ payload: sessionUpdate("other-session", "ignored") });
    // Two updates for our plugin session stream through in order.
    emit({ payload: sessionUpdate("plugin-session-1", "hi") });
    emit({ payload: sessionUpdate("plugin-session-1", "there") });
    await flush();
    // Resolving the prompt turn pushes the terminal event.
    resolvePrompt!({ stopReason: "end_turn" });

    await done;

    expect(events.map((event) => event.type)).toEqual([
      "session_update",
      "session_update",
      "completed",
    ]);
    expect(events.at(-1)).toEqual({ type: "completed", stopReason: "end_turn" });
    // The Ora session id was mapped to the plugin session id, and text was wrapped as a
    // ContentBlock text payload before forwarding to agent/prompt.
    expect(invokeCommand).toHaveBeenCalledWith(
      "plugin_agent_prompt",
      expect.objectContaining({
        pluginId: "opencode",
        request: expect.objectContaining({
          sessionId: "plugin-session-1",
          prompt: [{ type: "text", text: "hello" }],
        }),
      }),
    );
    expect(unlisten).toHaveBeenCalled();
  });

  it("cancels the plugin session when the prompt signal aborts", async () => {
    let handler: NotificationHandler | null = null;
    const unlisten = vi.fn();
    const listenForUpdates = vi.fn(async (_e: string, h: NotificationHandler) => {
      handler = h;
      return unlisten;
    });
    let resolvePrompt: ((r: acp.PromptResponse) => void) | null = null;
    const invokeCommand = vi.fn(async (command: string) => {
      if (command === "plugin_agent_new_session") return { sessionId: "ps" };
      if (command === "plugin_agent_prompt")
        return new Promise<acp.PromptResponse>((resolve) => {
          resolvePrompt = resolve;
        });
      if (command === "plugin_agent_cancel") return undefined;
      throw new Error(`unexpected: ${command}`);
    });

    const client = createTauriPluginChatSessionClient({
      pluginId: "opencode",
      cwd: "/tmp",
      invokeCommand: invokeCommand as never,
      listenForUpdates: listenForUpdates as never,
    });
    const controller = new AbortController();
    const events: PromptSessionEvent[] = [];
    const done = (async () => {
      for await (const event of client.prompt(
        { sessionId: "o1", text: "hi" },
        { signal: controller.signal },
      )) {
        events.push(event);
      }
    })();

    await vi.waitFor(() => expect(handler).not.toBeNull());
    controller.abort();
    await flush();
    // The plugin resolves the cancelled turn with stop_reason "cancelled".
    resolvePrompt!({ stopReason: "cancelled" });
    await done;

    expect(invokeCommand).toHaveBeenCalledWith(
      "plugin_agent_cancel",
      expect.objectContaining({ pluginId: "opencode", request: { sessionId: "ps" } }),
    );
    expect(events.at(-1)).toEqual({ type: "completed", stopReason: "cancelled" });
    expect(unlisten).toHaveBeenCalled();
  });
});
