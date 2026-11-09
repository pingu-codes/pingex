import { beforeEach, describe, expect, it } from "vitest";
import { appData } from "$lib/app/appData.svelte";
import {
  adoptThread,
  backLabel,
  currentProject,
  goBack,
  goHome,
  newThreadInDir,
  openGit,
  openProjectDetail,
  openPullRequests,
  openReview,
  openStackPage,
  openThread,
  openThreadInCwd,
  openWorktrees,
  selectedThreadInfo,
  setView,
  view,
} from "$lib/app/navigation.svelte";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
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
      inTempWorktree: false,
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
  unfiledThreads: [],
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

  it("opens worktrees and git as tabs of the project detail view", () => {
    openWorktrees(api);
    expect(view.detailPath).toBe("/projects/api");
    expect(view.detailTab).toBe("worktrees");
    openGit(api);
    expect(view.detailTab).toBe("git");
    openProjectDetail(api);
    expect(view.detailTab).toBe("overview");
  });

  it("clears the views a navigation does not name", () => {
    openWorktrees(api);
    expect(view.detailPath).toBe("/projects/api");

    openThread(api, "thread-1");
    expect(view.detailPath).toBeNull();
    expect(view.detailTab).toBeNull();
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
      inTempWorktree: true,
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

  it("opens the Pull requests inbox on the repository last shown, and a review on one PR", () => {
    setView({ projectPath: "/projects/api" });
    openPullRequests();
    expect([view.pullRequests, view.inboxPath]).toEqual([true, "/projects/api"]);
    openPullRequests("/projects/web");
    goHome();
    expect(view.pullRequests).toBe(false);
    openPullRequests();
    expect(view.inboxPath).toBe("/projects/web");

    openReview(api, 412);
    expect([view.pullRequests, view.reviewPath, view.reviewPr]).toEqual([false, "/projects/api", 412]);
  });

  describe("going back", () => {
    const stacked = (number: number, state = "open", merged = false) => ({
      number,
      state,
      merged,
      isDraft: false,
      headRef: `branch-${number}`,
      headSha: `sha-${number}`,
    });

    beforeEach(() => {
      delete inboxes["/projects/api"];
    });

    function withStack() {
      inboxes["/projects/api"] = {
        inbox: null,
        error: null,
        syncing: false,
        stacks: {
          supported: true,
          stacks: [{ number: 7, baseRef: "main", prs: [stacked(10), stacked(11), stacked(12, "closed", true)] }],
        },
      } as (typeof inboxes)[string];
    }

    it("returns from a PR to the inbox it was opened from", () => {
      openPullRequests("/projects/api");
      openReview(api, 412);
      expect(backLabel()).toBe("Pull requests");

      goBack();
      expect([view.pullRequests, view.inboxPath, view.reviewPr]).toEqual([true, "/projects/api", null]);
    });

    it("retraces a path through several pages, a step at a time", () => {
      openPullRequests("/projects/api");
      openStackPage("/projects/api", 7);
      openReview(api, 412);
      openReview(api, 413);
      expect(backLabel()).toBe("#412");

      goBack();
      expect(view.reviewPr).toBe(412);
      expect(backLabel()).toBe("Stack");

      goBack();
      expect([view.stackRepo, view.stackNumber, view.reviewPr]).toEqual(["/projects/api", 7, null]);
      expect(backLabel()).toBe("Pull requests");

      goBack();
      expect(view.pullRequests).toBe(true);
    });

    it("remounts the page it goes back to", () => {
      openPullRequests("/projects/api");
      openReview(api, 412);
      const before = view.epoch;
      goBack();
      expect(view.epoch).toBeGreaterThan(before);
      openReview(api, 412);
      expect(view.epoch).toBeGreaterThan(before + 1);
    });

    it("leaves no step behind for j/k through the queue", () => {
      openPullRequests("/projects/api");
      openReview(api, 412);
      openReview(api, 413, "walk");
      openReview(api, 414, "walk");
      expect(view.reviewPr).toBe(414);

      goBack();
      expect(view.pullRequests).toBe(true);
    });

    it("returns to a thread a PR was opened from", () => {
      openThread(api, "thread-1");
      openReview(api, 412);
      expect(backLabel()).toBe("Thread");

      goBack();
      expect(view.threadId).toBe("thread-1");
    });

    it("returns to the project a list of PRs was opened from", () => {
      openProjectDetail(api);
      openReview(api);
      expect(backLabel()).toBe("API");

      goBack();
      expect(view.detailPath).toBe("/projects/api");
    });

    it("opens a PR in a stack on the Stack page, on that PR", () => {
      withStack();
      openPullRequests("/projects/api");
      openReview(api, 11);
      expect([view.stackRepo, view.stackNumber, view.stackPr, view.reviewPath]).toEqual(["/projects/api", 7, 11, null]);

      goBack();
      expect(view.pullRequests).toBe(true);
    });

    it("opens a PR on its own page when it isn't open in a stack", () => {
      withStack();
      openReview(api, 12);
      expect([view.stackRepo, view.reviewPr]).toEqual([null, 12]);
      openReview(api, 412);
      expect([view.stackRepo, view.reviewPr]).toEqual([null, 412]);
    });

    it("comes back to a Stack page without restarting its restack", () => {
      openStackPage("/projects/api", 7, { restack: true });
      expect(view.stackRestack).toBe(true);
      openReview(api, 412);

      goBack();
      expect([view.stackNumber, view.stackRestack]).toEqual([7, false]);
    });

    it("starts again after going somewhere new", () => {
      openPullRequests("/projects/api");
      openReview(api, 412);
      openThread(api, "thread-1");
      openReview(api, 413);

      goBack();
      expect(view.threadId).toBe("thread-1");
      expect(backLabel()).toBe("Home");
    });

    it("falls back to the inbox with nothing to retrace", () => {
      setView({ projectPath: "/projects/api", reviewPath: "/projects/api", reviewPr: 412 });
      expect(backLabel()).toBe("Pull requests");

      goBack();
      expect([view.pullRequests, view.inboxPath]).toEqual([true, "/projects/api"]);
    });
  });
});
