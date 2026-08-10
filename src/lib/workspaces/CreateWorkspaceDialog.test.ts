import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Project } from "$lib/types";
import CreateWorkspaceDialog from "$lib/workspaces/CreateWorkspaceDialog.svelte";

function project(name: string, path: string): Project {
  return { name, path, kind: "folder", pinned: false, threads: [] };
}

const api = project("API", "/projects/api");
const web = project("Web", "/projects/web");
const workspace: Project = {
  name: "API + Web",
  path: "/home/user/.codex/multi-projects/workspace-1",
  kind: "multiProject",
  workspaceId: "workspace-1",
  pinned: false,
  threads: [],
  members: [
    {
      sourcePath: api.path,
      effectivePath: "/worktrees/api",
      alias: "server",
      isolated: true,
      available: true,
    },
    {
      sourcePath: web.path,
      effectivePath: web.path,
      alias: "client",
      isolated: false,
      available: true,
    },
  ],
};

function setup(overrides: Record<string, unknown> = {}) {
  const submit = vi.fn(async () => {});
  const close = vi.fn();
  render(CreateWorkspaceDialog, {
    projects: [api, web, workspace],
    submit,
    close,
    ...overrides,
  });
  return { submit, close };
}

describe("CreateWorkspaceDialog", () => {
  it("requires a name and two ordinary projects before creation", async () => {
    const user = userEvent.setup();
    const { submit } = setup();
    const createButton = screen.getByRole("button", { name: "Create workspace" });

    expect(createButton).toBeDisabled();
    await user.type(screen.getByLabelText("Workspace name"), "Frontend + API");
    await user.click(screen.getByRole("checkbox", { name: /API/ }));
    expect(createButton).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: /Web/ }));
    expect(createButton).toBeEnabled();
    await user.click(createButton);

    expect(submit).toHaveBeenCalledWith({
      name: "Frontend + API",
      members: [
        { sourcePath: "/projects/api", alias: "api", isolated: true },
        { sourcePath: "/projects/web", alias: "web", isolated: true },
      ],
    });
  });

  it("prepopulates an existing workspace and saves its edited membership", async () => {
    const user = userEvent.setup();
    const { submit } = setup({ workspace });

    expect(screen.getByDisplayValue("API + Web")).toBeInTheDocument();
    expect(screen.getByLabelText("Alias for API")).toHaveValue("server");
    expect(screen.getByLabelText("Alias for Web")).toHaveValue("client");
    await user.clear(screen.getByLabelText("Workspace name"));
    await user.type(screen.getByLabelText("Workspace name"), "Platform");
    await user.clear(screen.getByLabelText("Alias for Web"));
    await user.type(screen.getByLabelText("Alias for Web"), "frontend");
    await user.click(screen.getByRole("button", { name: "Save workspace" }));

    expect(submit).toHaveBeenCalledWith({
      name: "Platform",
      members: [
        { sourcePath: "/projects/api", alias: "server", isolated: true },
        { sourcePath: "/projects/web", alias: "frontend", isolated: false },
      ],
    });
  });
});
