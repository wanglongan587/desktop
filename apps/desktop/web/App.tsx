import { AppShell } from "@ora/app-shell";
import { createChatStore } from "@ora/chat";
import { createContractsClient } from "@ora/contracts";
import { createTauriPlatformAdapter } from "@ora/platform/tauri";
import { createTauriTransport } from "./tauri-transport";
import { createTauriPluginChatSessionClient } from "./tauri-chat-session-client";
import { DEFAULT_AGENT_PLUGIN_ID, useDefaultAgentPlugin } from "./use-default-agent-plugin";

const client = createContractsClient(createTauriTransport());
const chatStore = createChatStore(
  createTauriPluginChatSessionClient({ pluginId: DEFAULT_AGENT_PLUGIN_ID }),
);
const platform = createTauriPlatformAdapter();

export default function App() {
  // Activates the default agent plugin (scan → install → enable → activate) so the
  // chat session client can route prompts to it. Fire-and-forget at module load via
  // the hook's module-level promise; failures surface as a chat error on first prompt.
  useDefaultAgentPlugin();
  return <AppShell client={client} chatStore={chatStore} platform={platform} />;
}
