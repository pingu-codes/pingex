import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GitContext, Project } from "$lib/types";
import ProjectDetail from "./ProjectDetail.svelte";

const context = vi.fn<(dir: string) => Promise<GitContext>>();

vi.mock("$lib/services/api", () => ({
  isTauri: () => false,
  gitContext: (dir: string) => context(dir),
  gitStatus: vi.fn(async () => ({
    branch: "main",
    detached: false,
    upstream: "origin/main",
    ahead: 1,
    behind: 0,
    counts: { staged: 0, unstaged: 1, untracked: 0, conflicted: 0 },
    files: [{ path: "a.ts", state: "unstaged", code: ".M" }],
    truncated: false,
    refreshedAt: 0,
  })),
  gitWorktrees: vi.fn(async () => []),
  gitRepoInfo: vi.fn(async () => ({
    dir: "/repo",
    isGitRepo: true,
    root: "/repo",
    commonDir: "/repo/.git",
    branch: "main",
    detached: false,
    upstream: null,
    ahead: 0,
    behind: 0,
    inProgress: null,
    error: null,
  })),
  gitFileDiff: vi.fn(),
  gitStagedFileDiff: vi.fn(),
  gitStage: vi.fn(),
  gitUnstage: vi.fn(),
  gitDiscard: vi.fn(),
  gitCommit: vi.fn(),
  gitFetch: vi.fn(),
  gitPull: vi.fn(),
  gitPush: vi.fn(),
  listProjectSources: vi.fn(async () => []),
  addProjectSource: vi.fn(),
  removeProjectSource: vi.fn(),
  reindexSource: vi.fn(),
  revealInFinder: vi.fn(),
  saveProjectInstructions: vi.fn(),
  searchWorkspace: vi.fn(),
}));

vi.mock("$lib/services/codexEvents.svelte", () => ({ setThreadHandler: () => () => {} }));

function ctx(overrides: Partial<GitContext> = {}): GitContext {
  return {
    dir: "/repo",
    isGitRepo: true,
    kind: "main",
    parentPath: null,
    root: "/repo",
    commonDir: "/repo/.git",
    branch: "main",
    detached: false,
    upstream: "origin/main",
    ahead: 1,
    behind: 0,
    inProgress: null,
    identityName: "Test",
    identityEmail: "t@x",
    error: null,
    ...overrides,
  };
}

function project(overrides: Partial<Project> = {}): Project {
  return {
    name: "repo",
    path: "/repo",
    kind: "folder",
    workspaceId: null,
    pinned: false,
    archived: false,
    expanded: true,
    threads: [],
    instructions: "",
    sources: [],
    members: [],
    recencyAt: null,
    ...overrides,
  };
}

beforeEach(() => {
  context.mockReset();
  context.mockResolvedValue(ctx());
});

describe("ProjectDetail", () => {
  it("lands on the requested tab", async () => {
    render(ProjectDetail, { project: project(), initialTab: "worktrees", onOpenThread: vi.fn() });
    const tab = await screen.findByRole("tab", { name: "Worktrees" });
    await waitFor(() => expect(tab.getAttribute("aria-selected")).toBe("true"));
    expect(await screen.findByText(/Worktrees of/)).toBeTruthy();
  });

  it("shows the branch card on the overview and switches to the git tab", async () => {
    render(ProjectDetail, { project: project(), onOpenThread: vi.fn() });
    expect(await screen.findByText("origin/main", { exact: false })).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Open Git" }));
    expect(await screen.findByRole("button", { name: "Switch branch" })).toBeTruthy();
    expect(screen.getByRole("textbox", { name: "Commit message" })).toBeTruthy();
  });

  it("tells a linked worktree apart and links to its repository", async () => {
    context.mockResolvedValue(ctx({ dir: "/repo/wt", kind: "linked", parentPath: "/repo" }));
    const onOpenProjectPath = vi.fn();
    render(ProjectDetail, {
      project: project({ path: "/repo/wt", name: "wt" }),
      onOpenThread: vi.fn(),
      onOpenProjectPath,
    });
    expect(await screen.findByText(/This is a worktree of/)).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Open repository" }));
    expect(onOpenProjectPath).toHaveBeenCalledWith("/repo", "worktrees");
  });

  it("hides the git tabs for a workspace hub", () => {
    render(ProjectDetail, { project: project({ kind: "multiProject" }), onOpenThread: vi.fn() });
    expect(screen.queryByRole("tab", { name: "Git" })).toBeNull();
    expect(context).not.toHaveBeenCalled();
  });

  it("falls back to the overview when a folder is not a repository", async () => {
    context.mockResolvedValue(ctx({ isGitRepo: false, kind: "none" }));
    render(ProjectDetail, { project: project({ path: "/plain" }), initialTab: "git", onOpenThread: vi.fn() });
    await waitFor(() => expect(screen.queryByRole("tab", { name: "Git" })).toBeNull());
    expect(screen.getByRole("tab", { name: "Overview" }).getAttribute("aria-selected")).toBe("true");
  });
});
