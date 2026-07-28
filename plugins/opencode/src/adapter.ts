import type { acp } from "@ora/contracts";
import { methods, servePlugin } from "@ora-space/plugin-sdk";
import { AcpStdioClient } from "./acp-stdio-client";

/**
 * opencode agent plugin: bridges the Ora plugin channel (ACP-shaped `agent/*` methods) to a
 * real `opencode acp` subprocess (ACP `session/*` methods). Payloads are shared ACP types, so
 * this is a thin method-name proxy that also forwards `session/update` notifications back to
 * the host as `agent/sessionUpdate`.
 *
 * The opencode subprocess is spawned + ACP-initialized lazily on the host `initialize` handshake.
 */
let acpClient: AcpStdioClient | undefined;

async function client(): Promise<AcpStdioClient> {
  if (!acpClient) {
    acpClient = new AcpStdioClient({});
    await acpClient.initialize();
    // opencode advertises an auth method (opencode-login) but rejects the ACP `authenticate`
    // call with "Invalid params". Since the user runs `opencode auth login` out-of-band,
    // skip the ACP authenticate and rely on opencode's configured credentials for sessions.
    // If session/new later reports unauthorized, revisit the authenticate params.
  }
  return acpClient;
}

void servePlugin({
  [methods.initialize]: async () => {
    // Spawn + ACP-initialize opencode when the host activates the plugin.
    await client();
    return { kind: "agent", version: "0.1.0" };
  },
  [methods.agentNewSession]: async (params) =>
    (await client()).newSession(params as acp.NewSessionRequest),
  [methods.agentPrompt]: async (params, notify) => {
    const session = await client();
    return session.prompt(params as acp.PromptRequest, (notification) =>
      notify(methods.agentSessionUpdate, notification),
    );
  },
  [methods.agentCancel]: async (params) => {
    (await client()).cancel(params as acp.CancelNotification);
  },
  [methods.shutdown]: async () => {
    acpClient?.shutdown();
    process.exit(0);
  },
});
