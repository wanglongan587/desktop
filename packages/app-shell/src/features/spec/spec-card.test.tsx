import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { createChatStore, type ChatToolCall } from "@ora/chat";
import { TooltipProvider } from "@ora/ui";
import type { Project } from "@ora/contracts";
import { AppI18nProvider } from "../../i18n/i18n";
import { createMockClient, createMockClientState, type MockClientState } from "../../test/mock-client";
import { createHookWrapper, createTestQueryClient } from "../../test/hook-harness";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { useWorkspaceSelectionStore } from "../../state/stores/workspace-selection-store";
import { ToolCallBlock } from "../chat/tool-call-block";

const PROJECT: Project = { id: "p1", name: "Ora Desktop", rootPath: "/ora" };

/** Builds a workspace whose catalog declares one spec source. */
function workspaceWithSources(): MockClientState {
  const state = createMockClientState();
  state.projects = [PROJECT];
  state.specWorkspaceRoot = "/ora";
  state.specSources = [{ name: "OpenSpec", glob: "openspec/changes/**/*.md" }];
  return state;
}

/** Builds one completed write against the given path. */
function writeToolCall(path: string): ChatToolCall {
  return {
    kind: "toolCall",
    id: "tool-1",
    title: "Edit",
    toolKind: "edit",
    status: "completed",
    content: [],
    locations: [{ path }],
    createdAt: 1,
    updatedAt: 1,
  };
}

/** Renders one tool call with the provider stack the chat view supplies. */
function renderToolCall(state: MockClientState, tool: ChatToolCall) {
  const client = createMockClient(state);
  const Wrapper = createHookWrapper(client, createTestQueryClient(), createChatStore(client.session));
  return render(
    <Wrapper>
      <AppI18nProvider>
        <TooltipProvider>
          <ToolCallBlock tool={tool} />
        </TooltipProvider>
      </AppI18nProvider>
    </Wrapper>,
  );
}

beforeEach(() => {
  useWorkspaceSelectionStore.getState().selectProject(PROJECT.id);
  useSpecPanelStore.setState({ open: false, selectedPath: null });
});

describe("spec tool calls", () => {
  // The card only appears once the catalog's sources have loaded; before that the
  // same write is indistinguishable from any other edit.
  it("renders a spec card when a write lands inside a configured source", async () => {
    renderToolCall(workspaceWithSources(), writeToolCall("/ora/openspec/changes/add-auth/proposal.md"));

    expect(await screen.findByText("OpenSpec")).not.toBeNull();
    expect(screen.getByText("proposal.md")).not.toBeNull();
  });

  it("keeps an ordinary edit as a tool call block", async () => {
    renderToolCall(workspaceWithSources(), writeToolCall("/ora/crates/spec/src/lib.rs"));

    expect(await screen.findByText("lib.rs")).not.toBeNull();
    expect(screen.queryByText("OpenSpec")).toBeNull();
  });

  it("opens the panel on the written document when the card is activated", async () => {
    const user = userEvent.setup();
    renderToolCall(workspaceWithSources(), writeToolCall("/ora/openspec/changes/add-auth/proposal.md"));

    await screen.findByText("OpenSpec");
    await user.click(screen.getByRole("button", { name: /proposal\.md/ }));

    await waitFor(() =>
      expect(useSpecPanelStore.getState()).toMatchObject({
        open: true,
        selectedPath: "openspec/changes/add-auth/proposal.md",
      }),
    );
  });
});
