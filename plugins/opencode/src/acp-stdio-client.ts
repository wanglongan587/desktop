import { spawn, type ChildProcessByStdio } from "node:child_process";
import type { Readable, Writable } from "node:stream";
import type { acp } from "@ora/contracts";

/**
 * ACP stdio client: spawns `opencode acp` (an ACP-compatible subprocess speaking JSON-RPC
 * over stdio) and drives it as an ACP client.
 *
 * NOTE: unverified against a live `opencode acp` run. Two assumptions to confirm on first
 * live use: the ACP protocol version (`ACP_PROTOCOL_VERSION`) and the `clientCapabilities`
 * advertised in `initialize` (currently minimal). `opencode` handles its own auth/credentials
 * via its own config, so this client does not call `authenticate`.
 */
const ACP_PROTOCOL_VERSION = 1;

const METHODS = {
  initialize: "initialize",
  authenticate: "authenticate",
  sessionNew: "session/new",
  sessionPrompt: "session/prompt",
  sessionCancel: "session/cancel",
  sessionUpdate: "session/update",
} as const;

export interface AcpStdioClientOptions {
  /** Executable to run as the ACP agent; defaults to `opencode`. */
  command?: string;
  /** Args for the ACP agent; defaults to `["acp"]`. */
  args?: readonly string[];
  cwd?: string;
  env?: Record<string, string>;
}

interface PendingRequest {
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

/** Spawns an ACP agent over stdio and drives initialize / session/new / session/prompt / session/cancel. */
export class AcpStdioClient {
  private readonly child: ChildProcessByStdio<Writable, Readable, null>;
  private nextId = 1;
  private readonly pending = new Map<number, PendingRequest>();
  private buffer = "";
  private updateHandler: ((notification: acp.SessionNotification) => void) | undefined;
  private authMethods: acp.InitializeResponse["authMethods"] | undefined;

  constructor(options: AcpStdioClientOptions = {}) {
    this.child = spawn(options.command ?? "opencode", options.args ?? ["acp"], {
      cwd: options.cwd,
      env: { ...process.env, ...options.env },
      stdio: ["pipe", "pipe", "inherit"],
      // `opencode` ships as a `.cmd`/shim on Windows that `spawn` cannot resolve without a
      // shell; `shell: true` lets cmd/sh resolve it (transparent for line-delimited stdio).
      shell: true,
    });
    this.child.stdout.setEncoding("utf-8");
    this.child.stdout.on("data", (chunk: string) => this.onData(chunk));
    this.child.on("error", (error) => this.failAll(error));
    this.child.on("exit", () => this.failAll(new Error("ACP agent process exited")));
  }

  /** Performs the ACP `initialize` handshake; call before any session method. */
  async initialize(): Promise<acp.InitializeResponse> {
    const result = await this.request<acp.InitializeResponse>(METHODS.initialize, {
      protocolVersion: ACP_PROTOCOL_VERSION,
      // Minimal client capabilities: Ora does not expose fs/terminal surfaces to the agent.
      clientCapabilities: {} as unknown as acp.ClientCapabilities,
    });
    this.authMethods = result.authMethods;
    return result;
  }

  /**
   * Calls ACP `authenticate` with the first advertised auth method (e.g. opencode-login).
   *
   * Agents that advertise auth methods expect this call before session ops. If the agent is
   * already authenticated out-of-band (e.g. `opencode auth login` run in a terminal), this
   * resolves immediately; otherwise the agent returns auth-required instructions and a later
   * `session/new` will surface the unauthorized failure.
   */
  async authenticate(): Promise<void> {
    const firstId = (this.authMethods?.[0] as unknown as { id?: string } | undefined)?.id;
    if (!firstId) return;
    await this.request(METHODS.authenticate, { method: firstId });
  }

  /** Opens an ACP session. */
  async newSession(request: acp.NewSessionRequest): Promise<acp.NewSessionResponse> {
    return this.request<acp.NewSessionResponse>(METHODS.sessionNew, request);
  }

  /** Sends a prompt turn; `session/update` notifications stream to `onUpdate` before the response resolves. */
  async prompt(
    request: acp.PromptRequest,
    onUpdate: (notification: acp.SessionNotification) => void,
  ): Promise<acp.PromptResponse> {
    this.updateHandler = onUpdate;
    try {
      return await this.request<acp.PromptResponse>(METHODS.sessionPrompt, request);
    } finally {
      this.updateHandler = undefined;
    }
  }

  /** Cancels the in-flight prompt for a session (ACP notification, no response). */
  cancel(notification: acp.CancelNotification): void {
    this.notify(METHODS.sessionCancel, notification);
  }

  /** Closes stdin and terminates the agent process. */
  shutdown(): void {
    this.child.stdin.end();
    this.child.kill();
  }

  private request<T>(method: string, params: unknown): Promise<T> {
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      this.pending.set(id, {
        resolve: resolve as (value: unknown) => void,
        reject,
      });
      this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
    });
  }

  private notify(method: string, params: unknown): void {
    this.child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method, params })}\n`);
  }

  private onData(chunk: string): void {
    this.buffer += chunk;
    for (;;) {
      const idx = this.buffer.indexOf("\n");
      if (idx < 0) break;
      const line = this.buffer.slice(0, idx);
      this.buffer = this.buffer.slice(idx + 1);
      this.handleLine(line);
    }
  }

  private handleLine(line: string): void {
    const trimmed = line.trim();
    if (trimmed.length === 0) return;
    let message: { id?: number; method?: string; result?: unknown; error?: { message?: string }; params?: unknown };
    try {
      message = JSON.parse(trimmed);
    } catch {
      return; // skip non-JSON lines (agent diagnostics on stderr, which is inherited)
    }
    if (typeof message.id === "number") {
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      if (message.error) {
        pending.reject(new Error(message.error.message ?? "ACP agent error"));
      } else {
        pending.resolve(message.result);
      }
    } else if (message.method === METHODS.sessionUpdate && message.params) {
      this.updateHandler?.(message.params as acp.SessionNotification);
    }
  }

  private failAll(error: Error): void {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}
