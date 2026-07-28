import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { homeDir } from "@tauri-apps/api/path";
import type { ChatSessionClient } from "@ora/chat";
import type {
  acp,
  ContractCallOptions,
  LoadSessionEvent,
  LoadSessionRequest,
  PromptSessionEvent,
  PromptSessionRequest,
  RespondToPermissionRequest,
  RespondToPermissionResponse,
} from "@ora/contracts";

/** Tauri event the plugin runtime emits for every `agent/sessionUpdate` notification. */
const AGENT_SESSION_UPDATE_EVENT = "agent/sessionUpdate";

export interface TauriPluginChatSessionClientOptions {
  /** Plugin id whose activated runtime carries the agent conversation. */
  pluginId: string;
  /**
   * Working directory the agent session operates in. Resolved lazily to the user's
   * home directory when omitted. Wire this to the selected project root to make the
   * agent operate on a real repository.
   */
  cwd?: string;
  /** Injectable for tests; defaults to the Tauri core `invoke`. */
  invokeCommand?: typeof invoke;
  /** Injectable for tests; defaults to the Tauri event `listen`. */
  listenForUpdates?: typeof listen;
}

/**
 * Bridges the Ora chat domain to an active agent plugin via Tauri commands
 * (`plugin_agent_new_session` / `plugin_agent_prompt` / `plugin_agent_cancel`)
 * and the `agent/sessionUpdate` event stream the plugin runtime emits.
 *
 * Implements {@link ChatSessionClient} so `createChatStore` can route prompts to a
 * plugin instead of the shared backend's native agent-CLI path. Ora session ids map
 * lazily to plugin ACP session ids on first prompt; the mapping persists for the
 * conversation's lifetime so multi-turn continuity is preserved across the wire.
 *
 * `load` and `respondToPermission` are intentionally minimal: the plugin-channel
 * contract does not yet expose history replay or permission forwarding, so `load`
 * reports an empty completed turn and permission responses raise a clear error.
 * Both are extension points — add `agent/loadSession` / `agent/respondToPermission`
 * to the plugin contract and stream them through here.
 */
export function createTauriPluginChatSessionClient(
  options: TauriPluginChatSessionClientOptions,
): ChatSessionClient {
  const invokeCommand = options.invokeCommand ?? invoke;
  const listenForUpdates = options.listenForUpdates ?? listen;
  /** Ora session id → plugin ACP session id, created lazily on first prompt. */
  const pluginSessions = new Map<string, string>();

  async function resolveCwd(): Promise<string> {
    return options.cwd ?? (await homeDir());
  }

  async function ensurePluginSession(oraSessionId: string): Promise<string> {
    const existing = pluginSessions.get(oraSessionId);
    if (existing) return existing;
    const response = await invokeCommand<acp.NewSessionResponse>(
      "plugin_agent_new_session",
      { pluginId: options.pluginId, request: { cwd: await resolveCwd(), mcpServers: [] } },
    );
    pluginSessions.set(oraSessionId, response.sessionId);
    return response.sessionId;
  }

  async function* streamPrompt(
    request: PromptSessionRequest,
    signal: AbortSignal | undefined,
  ): AsyncGenerator<PromptSessionEvent> {
    const pluginSessionId = await ensurePluginSession(request.sessionId);

    // Merge the push-based Tauri event stream with the terminal invoke promise into
    // one pull-based async iterator. `push` wakes a waiting consumer or buffers.
    const queue: PromptSessionEvent[] = [];
    let waiter: ((event: PromptSessionEvent | null) => void) | null = null;
    let settled = false;

    const push = (event: PromptSessionEvent): void => {
      if (settled) return;
      if (waiter !== null) {
        const resolve = waiter;
        waiter = null;
        resolve(event);
      } else {
        queue.push(event);
      }
    };
    const next = (): Promise<PromptSessionEvent | null> =>
      queue.length > 0
        ? Promise.resolve(queue.shift() ?? null)
        : new Promise((resolve) => {
            waiter = resolve;
          });

    const unlisten = await listenForUpdates<acp.SessionNotification>(
      AGENT_SESSION_UPDATE_EVENT,
      (event) => {
        const notification = event.payload;
        if (notification.sessionId !== pluginSessionId) return;
        push({ type: "session_update", update: notification.update });
      },
    );

    // On abort, ask the plugin to cancel; the prompt invoke then resolves/rejects
    // and pushes the terminal event.
    let onAbort: (() => void) | null = null;
    if (signal) {
      onAbort = () => {
        if (settled) return;
        void invokeCommand<void>("plugin_agent_cancel", {
          pluginId: options.pluginId,
          request: { sessionId: pluginSessionId },
        }).catch(() => {});
      };
      signal.addEventListener("abort", onAbort);
    }

    invokeCommand<acp.PromptResponse>("plugin_agent_prompt", {
      pluginId: options.pluginId,
      request: {
        sessionId: pluginSessionId,
        prompt: [{ type: "text", text: request.text }],
      },
    })
      .then((response) => push({ type: "completed", stopReason: response.stopReason }))
      .catch(() => push({ type: "completed", stopReason: "cancelled" }));

    try {
      while (true) {
        const event = await next();
        if (event === null || event.type === "completed") {
          if (event?.type === "completed") yield event;
          break;
        }
        yield event;
      }
    } finally {
      settled = true;
      if (signal && onAbort) signal.removeEventListener("abort", onAbort);
      unlisten();
    }
  }

  return {
    load: async function* (_request: LoadSessionRequest): AsyncGenerator<LoadSessionEvent> {
      // History replay is not yet in the plugin-channel contract; the Ora chat store
      // keeps the live conversation in memory, so a reload reports an empty turn.
      yield { type: "completed" };
    },
    prompt: (request: PromptSessionRequest, options?: ContractCallOptions) =>
      streamPrompt(request, options?.signal),
    respondToPermission: async (
      _request: RespondToPermissionRequest,
    ): Promise<RespondToPermissionResponse> => {
      throw new Error(
        "plugin bridge does not yet forward permission responses; add agent/respondToPermission to the plugin-channel contract",
      );
    },
  };
}
