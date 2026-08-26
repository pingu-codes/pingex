import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Project } from "$lib/types";
import MoveToWorkspaceDialog from "$lib/workspaces/MoveToWorkspaceDialog.svelte";

const folder: Project = {
  name: "API",
  path: "/projects/api",
  kind: "folder",
  workspaceId: null,
  archived: false,
  instructions: "",
  sources: [],
  pinned: false,
  expanded: true,
  threads: [],
};
const workspace: Project = {
  name: "API + Web",
  path: "/home/user/.codex/multi-projects/workspace-1",
  kind: "multiProject",
  archived: false,
  instructions: "",
  sources: [],
  workspaceId: "workspace-1",
  pinned: false,
  expanded: true,
  threads: [],
  members: [
    {
      sourcePath: "/projects/api",
      effectivePath: "/projects/api",
      alias: "api",
      branch: null,
      isolated: false,
      available: true,
    },
    {
      sourcePath: "/projects/web",
      effectivePath: "/projects/web",
      alias: "web",
      branch: null,
      isolated: false,
      available: true,
    },
  ],
};

describe("MoveToWorkspaceDialog", () => {
  it("lists active workspaces only and returns the selected workspace", async () => {
    const user = userEvent.setup();
    const close = vi.fn();
    render(MoveToWorkspaceDialog, { workspaces: [folder, workspace], close });

    expect(screen.queryByText("API", { selector: "button" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /API \+ Web/ }));
    expect(close).toHaveBeenCalledWith(workspace);
  });

  it("explains how to proceed when there is no workspace", () => {
    render(MoveToWorkspaceDialog, { workspaces: [folder], close: vi.fn() });
    expect(screen.getByText("Create a workspace before moving this thread.")).toBeVisible();
  });
});
