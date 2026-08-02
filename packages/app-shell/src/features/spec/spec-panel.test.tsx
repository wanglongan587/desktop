import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { createChatStore } from "@ora/chat";
import { TooltipProvider } from "@ora/ui";
import type { Project } from "@ora/contracts";
import { AppI18nProvider } from "../../i18n/i18n";
import { createMockClient, createMockClientState, type MockClientState } from "../../test/mock-client";
import { createHookWrapper, createTestQueryClient } from "../../test/hook-harness";
import { useSpecPanelStore } from "../../state/stores/spec-panel-store";
import { useWorkspaceSelectionStore } from "../../state/stores/workspace-selection-store";
import { SpecPanel } from "./spec-panel";

const PROJECT: Project = { id: "p1", name: "Ora Desktop", rootPath: "/ora" };

/** Builds a workspace holding one document per configured source. */
function workspaceWithSpecs(): MockClientState {
  const state = createMockClientState();
  state.projects = [PROJECT];
  state.specWorkspaceRoot = "/ora";
  state.specSources = [
    { name: "OpenSpec", glob: "openspec/changes/**/*.md" },
    { name: "Docs", glob: "docs/specs/*.md" },
    { name: "Empty", glob: "design/*.md" },
  ];
  state.specs = [
    { id: "add-auth", sourceName: "OpenSpec", path: "openspec/changes/add-auth/proposal.md", title: "Add auth" },
    { id: "docs/specs/layout.md", sourceName: "Docs", path: "docs/specs/layout.md", title: "Layout" },
  ];
  state.specContents = {
    "docs/specs/layout.md": "# Layout\n\nThe shell has three columns.\n",
  };
  return state;
}

/** Renders the panel with the provider stack AppShell gives it. */
function renderPanel(state: MockClientState) {
  const client = createMockClient(state);
  const Wrapper = createHookWrapper(client, createTestQueryClient(), createChatStore(client.session));
  return render(
    <Wrapper>
      <AppI18nProvider>
        <TooltipProvider>
          <SpecPanel />
        </TooltipProvider>
      </AppI18nProvider>
    </Wrapper>,
  );
}

beforeEach(() => {
  useWorkspaceSelectionStore.getState().clearSelection();
  useSpecPanelStore.setState({ open: true, selectedPath: null });
});

describe("SpecPanel", () => {
  it("asks for a project before reading any catalog", async () => {
    renderPanel(workspaceWithSpecs());

    expect(await screen.findByText(/请先选择一个项目|Pick a project/)).not.toBeNull();
  });

  it("groups documents under their source and hides sources with none", async () => {
    useWorkspaceSelectionStore.getState().selectProject(PROJECT.id);
    renderPanel(workspaceWithSpecs());

    expect(await screen.findByText("Add auth")).not.toBeNull();
    expect(screen.getByText("Layout")).not.toBeNull();
    expect(screen.getByRole("heading", { name: /OpenSpec/ })).not.toBeNull();
    expect(screen.queryByRole("heading", { name: /Empty/ })).toBeNull();
  });

  it("renders the selected document as markdown", async () => {
    const user = userEvent.setup();
    useWorkspaceSelectionStore.getState().selectProject(PROJECT.id);
    renderPanel(workspaceWithSpecs());

    await user.click(await screen.findByRole("button", { name: /Layout/ }));

    expect(await screen.findByRole("heading", { name: "Layout", level: 1 })).not.toBeNull();
    expect(screen.getByText("The shell has three columns.")).not.toBeNull();
  });

  // A path can point at a document the current workspace does not contain, for
  // example after switching to a branch where the file was never created.
  it("falls back to the empty reader when the selected path is not in the catalog", async () => {
    useWorkspaceSelectionStore.getState().selectProject(PROJECT.id);
    useSpecPanelStore.setState({ open: true, selectedPath: "docs/specs/removed.md" });
    renderPanel(workspaceWithSpecs());

    expect(await screen.findByText(/从左侧选择一篇 spec|Choose a spec/)).not.toBeNull();
  });

  it("closes the panel from its header control", async () => {
    const user = userEvent.setup();
    useWorkspaceSelectionStore.getState().selectProject(PROJECT.id);
    renderPanel(workspaceWithSpecs());

    await user.click(await screen.findByRole("button", { name: /收起 Spec 面板|Collapse the spec panel/ }));

    await waitFor(() => expect(useSpecPanelStore.getState().open).toBe(false));
  });
});
