import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  EnablePluginResponse,
  InstallPluginResponse,
  ListPluginsResponse,
  Plugin,
  PluginState,
  ScanPluginsResponse,
} from "@ora/contracts";

/**
 * Plugin id of the default agent plugin the desktop chat routes to. The matching
 * manifest lives at `plugins/opencode/plugin.json`; the desktop's `ORA_PLUGINS_ROOT`
 * must point at a directory containing it.
 */
export const DEFAULT_AGENT_PLUGIN_ID = "opencode";

export type DefaultAgentPluginStatus = "loading" | "ready" | "error";

/**
 * Ensures the default agent plugin (`DEFAULT_AGENT_PLUGIN_ID`) is discovered, installed,
 * enabled, and activated so the chat session client can route `newSession`/`prompt`/`cancel`
 * to it.
 *
 * Idempotent: install/enable are skipped when already done, activation tolerates
 * `plugin_already_active`, and a module-level promise dedups React StrictMode's dev
 * double-invoke (which would otherwise race two concurrent installs into a UNIQUE error).
 * Desktop-only (shells out to Tauri plugin commands).
 */
export function useDefaultAgentPlugin(): DefaultAgentPluginStatus {
  const [status, setStatus] = useState<DefaultAgentPluginStatus>("loading");
  useEffect(() => {
    let cancelled = false;
    void setupPromise.then((result) => {
      if (!cancelled) setStatus(result);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return status;
}

// Module-level: the module evaluates once per page load, so this starts a single setup run
// that every mount (including StrictMode's double-invoke) attaches to — no racing installs.
const setupPromise: Promise<DefaultAgentPluginStatus> = ensureDefaultAgentPlugin();

async function ensureDefaultAgentPlugin(): Promise<DefaultAgentPluginStatus> {
  try {
    // List first (cheap) — if already installed (e.g. a prior mount), skip scan + install.
    let installed: Plugin | undefined = (
      await invoke<ListPluginsResponse>("list_plugins", { request: {} })
    ).plugins.find((p) => p.id === DEFAULT_AGENT_PLUGIN_ID);

    if (!installed) {
      const scanned = await invoke<ScanPluginsResponse>("scan_plugins", { request: {} });
      const discovered = scanned.plugins.find(
        (p) => p.manifest.id === DEFAULT_AGENT_PLUGIN_ID,
      );
      if (!discovered) {
        throw new Error(
          `default agent plugin "${DEFAULT_AGENT_PLUGIN_ID}" not found under the plugins root`,
        );
      }
      try {
        installed = (
          await invoke<InstallPluginResponse>("install_plugin", {
            request: { plugin: discovered },
          })
        ).plugin;
      } catch (error) {
        // UNIQUE constraint = a concurrent install (StrictMode) already inserted it; re-fetch.
        const message = (error as { message?: string } | null)?.message ?? "";
        if (!/UNIQUE constraint/i.test(message)) throw error;
        const reList = await invoke<ListPluginsResponse>("list_plugins", { request: {} });
        installed = reList.plugins.find((p) => p.id === DEFAULT_AGENT_PLUGIN_ID);
        if (!installed) throw error;
      }
    }

    // Only Installed→Enabled is a legal metadata transition; leave Enabled/Started/Activated.
    if (installed.state === ("installed" satisfies PluginState)) {
      const enabledResp = await invoke<EnablePluginResponse>("enable_plugin", {
        request: { pluginId: DEFAULT_AGENT_PLUGIN_ID },
      });
      installed = enabledResp.plugin;
    }

    // Activate (spawn the plugin process). Already-active is fine on remount.
    try {
      await invoke("plugin_activate", { plugin: installed });
    } catch (error) {
      if ((error as { code?: string } | null)?.code !== "plugin_already_active") {
        throw error;
      }
    }

    return "ready";
  } catch (error) {
    console.error("[default-agent-plugin] setup failed:", error);
    return "error";
  }
}
