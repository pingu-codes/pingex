import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import Sidebar from "$lib/layout/Sidebar.svelte";
import { sidebarPrefs } from "$lib/layout/sidebarPrefs.svelte";
import { threadSummary, worktreeEntry } from "$lib/services/preview/factories";
import type { Project, ThreadSummary } from "$lib/types";
import { resetWorktreeMap, worktreeMap } from "$lib/worktrees/worktreeMap.svelte";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      setProjectExpanded: vi.fn(() => Promise.resolve(null)),
      gitStatus: vi.fn(() => Promise.reject(new Error("not in tests"))),
      gitWorktrees: vi.fn((repo: string) =>
        Promise.resolve(
          repo === "/repo"
            ? [
                worktreeEntry({ path: "/repo", isMain: true, branch: "main" }),
                worktreeEntry({ path: "/wt/adopted", branch: "adopted-branch" }),
                worktreeEntry({ path: "/elsewhere/loose", branch: "loose-branch" }),
              ]
            : [],
        ),
      ),
    },
  };
});

function project(path: string, name: string, overrides: Partial<Project> = {}): Project {
  return {
    path,
    name,
    kind: "folder",
    workspaceId: null,
    archived: false,
    instructions: "",
    sources: [],
    pinned: false,
    expanded: true,
    threads: [],
    ...overrides,
  };
}

const thread = (id: string, cwd: string): ThreadSummary => threadSummary({ id, title: id, cwd });

function setup() {
  const onNewThreadInDir = vi.fn();
  const onAddWorktreeProject = vi.fn();
  const onSelectThread = vi.fn();
  const projects = $state([
    project("/repo", "repo", { threads: [thread("Repo thread", "/repo")] }),
    project("/wt/adopted", "Adopted worktree", {
      kind: "worktree",
      threads: [thread("Adopted thread", "/wt/adopted")],
    }),
  ]);
  render(Sidebar, {
    projects,
    account: null,
    selectedThread: null,
    loading: false,
    onAddProject: vi.fn(),
    onSelectThread,
    onNewThread: vi.fn(),
    onOpenSettings: vi.fn(),
    onMenuAction: vi.fn(),
    unfiledThreads: [thread("Loose thread", "/elsewhere/loose")],
    onNewThreadInDir,
    onAddWorktreeProject,
  });
  return { onNewThreadInDir, onAddWorktreeProject, onSelectThread, projects };
}

describe("Sidebar worktree nesting", () => {
  afterEach(() => {
    sidebarPrefs.setNestWorktrees(false);
    resetWorktreeMap();
  });

  it("lists worktrees as separate projects until nesting is switched on", async () => {
    setup();
    expect(screen.queryByTestId("unadded-worktree")).toBeNull();
    expect(screen.queryByText("Loose thread")).toBeNull();

    await userEvent.click(screen.getByTestId("nest-worktrees"));
    const loose = await screen.findByTestId("unadded-worktree");
    expect(within(loose).getByText("loose")).toBeTruthy();
    expect(screen.getByTestId("nest-worktrees").getAttribute("aria-pressed")).toBe("true");
    // The adopted worktree is still drawn, now inside the repository's row.
    expect(screen.getByText("Adopted worktree")).toBeTruthy();
    expect(worktreeMap.unaddedParents).toEqual({ "/elsewhere/loose": "/repo" });
  });

  it("opens an unadded worktree's threads and starts new ones in it", async () => {
    sidebarPrefs.setNestWorktrees(true);
    const { onNewThreadInDir, onAddWorktreeProject, onSelectThread, projects } = setup();
    const row = await screen.findByTestId("unadded-worktree");

    await userEvent.click(within(row).getByText("loose"));
    await userEvent.click(await screen.findByText("Loose thread"));
    expect(onSelectThread).toHaveBeenCalledWith(projects[0], expect.any(String));

    await userEvent.click(within(row).getByRole("button", { name: "New thread in loose" }));
    expect(onNewThreadInDir).toHaveBeenCalledWith("/elsewhere/loose");
    await userEvent.click(within(row).getByRole("button", { name: "Add to sidebar" }));
    expect(onAddWorktreeProject).toHaveBeenCalledWith("/elsewhere/loose");
  });

  it("keeps the repository's own threads on its row", async () => {
    sidebarPrefs.setNestWorktrees(true);
    setup();
    await screen.findByTestId("unadded-worktree");
    await waitFor(() => expect(screen.getByText("Repo thread")).toBeTruthy());
    expect(screen.getByText("Adopted thread")).toBeTruthy();
  });
});
