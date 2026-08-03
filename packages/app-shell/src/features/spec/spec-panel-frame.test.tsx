import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createChatStore, type ChatToolCall } from "@ora/chat";
import { TooltipProvider } from "@ora/ui";
import type { Project } from "@ora/contracts";
import { AppI18nProvider } from "../../i18n/i18n";
import { createMockClient, createMockClientState, type MockClientState } from "../../test/mock-client";
import { createHookWrapper, createTestQueryClient } from "../../test/hook-harness";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { useWorkspaceSelectionStore } from "../../state/stores/workspace-selection-store";
import { ResponseTurn } from "../chat/response-turn";
import { SpecPanelFrame } from "./spec-panel-frame";
import { SPEC_PANEL_DEFAULT_WIDTH } from "../../lib/spec-panel-layout";

const PROJECT: Project = { id: "p1", name: "Ora Desktop", rootPath: "/ora" };

/** Builds a workspace whose catalog declares the OpenSpec source. */
function workspaceWithSources(): MockClientState {
  const state = createMockClientState();
  state.projects = [PROJECT];
  state.specWorkspaceRoot = "/ora";
  state.specSources = [{ name: "OpenSpec", glob: "openspec/changes/**/*.md" }];
  state.specs = [
    {
      id: "openspec/changes/add-auth/proposal.md",
      sourceName: "OpenSpec",
      path: "openspec/changes/add-auth/proposal.md",
      title: "Add auth",
    },
  ];
  state.specContents = {
    "openspec/changes/add-auth/proposal.md": "# Add auth\n",
  };
  return state;
}

/** Builds one completed write against the given path. */
function writeToolCall(id: string, path: string): ChatToolCall {
  return {
    kind: "toolCall",
    id,
    title: "Edit",
    toolKind: "edit",
    status: "completed",
    content: [],
    locations: [{ path }],
    createdAt: 1,
    updatedAt: 1,
  };
}

/** Renders a response turn beside the Spec frame under the real provider stack. */
function renderTurnWithFrame(state: MockClientState, items: ChatToolCall[]) {
  const client = createMockClient(state);
  const Wrapper = createHookWrapper(client, createTestQueryClient(), createChatStore(client.session));
  return render(
    <Wrapper>
      <AppI18nProvider>
        <TooltipProvider>
          <div className="flex h-96 w-[1600px]">
            <ResponseTurn
              userName="Ada"
              turn={{
                id: "turn-1",
                status: "completed",
                items,
              }}
            />
            <SpecPanelFrame />
          </div>
        </TooltipProvider>
      </AppI18nProvider>
    </Wrapper>,
  );
}

beforeEach(() => {
  useWorkspaceSelectionStore.getState().selectProject(PROJECT.id);
  useSpecPanelStore.setState({
    open: false,
    selectedPath: null,
    panelWidth: SPEC_PANEL_DEFAULT_WIDTH,
  });
  Object.defineProperty(document.documentElement, "clientWidth", {
    configurable: true,
    value: 1600,
  });
  vi.stubGlobal(
    "matchMedia",
    (query: string) =>
      ({
        matches: query.includes("prefers-reduced-motion"),
        media: query,
        onchange: null,
        addEventListener: () => {},
        removeEventListener: () => {},
        addListener: () => {},
        removeListener: () => {},
        dispatchEvent: () => false,
      }) satisfies MediaQueryList,
  );
});

describe("SpecPanelFrame reveal from chat", () => {
  it("opens a visible frame when a standalone Spec card is activated", async () => {
    const user = userEvent.setup();
    renderTurnWithFrame(workspaceWithSources(), [
      writeToolCall("tool-1", "/ora/openspec/changes/add-auth/proposal.md"),
    ]);

    await user.click(await screen.findByRole("button", { name: /打开 Spec：proposal\.md|Open Spec: proposal\.md/ }));

    await waitFor(() => {
      expect(useSpecPanelStore.getState()).toMatchObject({
        open: true,
        selectedPath: "openspec/changes/add-auth/proposal.md",
      });
    });
    await waitFor(() => {
      expect(screen.getByTestId("spec-panel-frame")).toHaveStyle({
        width: `${SPEC_PANEL_DEFAULT_WIDTH}px`,
      });
    });
  });

  it("keeps a Spec write as its own card even when adjacent to other edits", async () => {
    const user = userEvent.setup();
    renderTurnWithFrame(workspaceWithSources(), [
      writeToolCall("tool-1", "/ora/crates/spec/src/lib.rs"),
      writeToolCall("tool-2", "/ora/openspec/changes/add-auth/proposal.md"),
      writeToolCall("tool-3", "/ora/crates/spec/src/catalog.rs"),
    ]);

    const card = await screen.findByRole("button", { name: /打开 Spec：proposal\.md|Open Spec: proposal\.md/ });
    expect(screen.queryByText(/已修改 3 个文件|Changed 3 files/)).toBeNull();

    await user.click(card);
    await waitFor(() => expect(useSpecPanelStore.getState().open).toBe(true));
    await waitFor(() => {
      const width = screen.getByTestId("spec-panel-frame").style.width;
      expect(width).not.toBe("0px");
      expect(width).not.toBe("");
    });
  });
});

describe("SpecPanelFrame open animation", () => {
  it("reaches the remembered width after revealSpec", async () => {
    const state = workspaceWithSources();
    const client = createMockClient(state);
    const Wrapper = createHookWrapper(client, createTestQueryClient(), createChatStore(client.session));

    render(
      <Wrapper>
        <AppI18nProvider>
          <TooltipProvider>
            <SpecPanelFrame />
          </TooltipProvider>
        </AppI18nProvider>
      </Wrapper>,
    );

    act(() => {
      useSpecPanelStore.getState().revealSpec("openspec/changes/add-auth/proposal.md");
    });

    await waitFor(() => {
      expect(screen.getByTestId("spec-panel-frame")).toHaveStyle({
        width: `${SPEC_PANEL_DEFAULT_WIDTH}px`,
      });
    });
  });
});
