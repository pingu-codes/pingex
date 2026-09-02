import { beforeEach, describe, expect, it } from "vitest";
import { appData } from "$lib/app/appData.svelte";
import {
  adoptThread,
  currentProject,
  goHome,
  newThreadInDir,
  openThread,
  openThreadInCwd,
  openWorktrees,
  selectedThreadInfo,
  setView,
  view,
} from "$lib/app/navigation.svelte";
import { isTouched, resetTouched } from "$lib/layout/sessionFocus.svelte";
import type { BootstrapData, Project } from "$lib/types";

const api: Project = {
  name: "API",
  path: "/projects/api",
  kind: "folder",
  workspaceId: null,
  archived: false,
  instructions: "",
  sources: [],
  pinned: false,
  expanded: true,
  threads: [
    {
      id: "thread-1",
      cwd: "/projects/api",
      title: "Fix the parser",
      updatedAt: 0,
      status: "idle",
      pinned: false,
      parentThreadId: null,
      agentNickname: null,
      agentRole: null,
      projectId: null,
      sectionId: null,
      subagentCount: 0,
      hidden: false,
    },
  ],
};

const data: BootstrapData = {
  codexHome: "/home/.codex",
  codexBinary: "codex",
  projects: [api],
  account: null,
  sideQuestions: [],
  threadBranches: [],
  subagents: [],
  sections: [],
  sectionsSupported: false,
  sidebarLayout: { folders: [], placements: [] },
};

describe("navigation", () => {
  beforeEach(() => {
    appData.data = structuredClone(data);
    goHome();
  });

  it("marks an opened thread as touched for session focus", () => {
    resetTouched();
    expect(isTouched("thread-1")).toBe(false);
    openThread(api, "thread-1");
    expect(isTouched("thread-1")).toBe(true);
    goHome();
    expect(isTouched("thread-1")).toBe(true);
  });

  it("clears the views a navigation does not name", () => {
    openWorktrees(api);
    expect(view.worktreesPath).toBe("/projects/api");

    openThread(api, "thread-1");
    expect(view.worktreesPath).toBeNull();
    expect(view.draftCwd).toBeNull();
    expect(view.threadId).toBe("thread-1");
    expect(view.projectPath).toBe("/projects/api");
  });

  it("remounts on navigation but not when a draft becomes a thread", () => {
    const before = view.epoch;
    openThread(api, "thread-1");
    expect(view.epoch).toBe(before + 1);

    newThreadInDir("/projects/api/wt-feature");
    const drafting = view.epoch;
    adoptThread("thread-2");
    // The turn is already running in this view; remounting would restart it.
    expect(view.epoch).toBe(drafting);
    expect(view.threadId).toBe("thread-2");
    expect(view.draftCwd).toBeNull();
  });

  it("resolves projects and threads through the current bootstrap data", () => {
    openThread(api, "thread-1");
    expect(currentProject()?.name).toBe("API");
    expect(selectedThreadInfo()?.title).toBe("Fix the parser");

    // A refresh that renames the project is picked up without re-navigating.
    const renamed = structuredClone(data);
    renamed.projects[0].name = "API service";
    appData.data = renamed;
    expect(currentProject()?.name).toBe("API service");
  });

  it("opens a thread running in a temporary worktree under the project that lists it", () => {
    const withWorktreeThread = structuredClone(data);
    withWorktreeThread.projects[0].threads.push({
      id: "thread-tmp",
      cwd: "/home/.codex/worktrees-tmp/api/abc123",
      title: "Try an idea",
      updatedAt: 0,
      status: "idle",
      pinned: false,
      parentThreadId: null,
      agentNickname: null,
      agentRole: null,
      projectId: null,
      sectionId: null,
      subagentCount: 0,
      hidden: false,
    });
    appData.data = withWorktreeThread;

    openThreadInCwd("thread-tmp", "/home/.codex/worktrees-tmp/api/abc123");
    expect(view.threadId).toBe("thread-tmp");
    expect(view.projectPath).toBe("/projects/api");
  });

  it("keeps the current project when a directory matches no project", () => {
    setView({ projectPath: "/projects/api" });
    newThreadInDir("/tmp/scratch");
    expect(view.draftCwd).toBe("/tmp/scratch");
    expect(view.projectPath).toBe("/projects/api");
  });
});
