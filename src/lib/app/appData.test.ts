import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  appData,
  applyData,
  applyTicketed,
  dataTicket,
  forgetOptimisticThread,
  nameNewThread,
  projectForCwd,
  quietRefresh,
  refresh,
  trackNewThread,
  UNTITLED_THREAD,
} from "$lib/app/appData.svelte";
import { commands } from "$lib/services/homeRouting";
import type { BootstrapData, Project } from "$lib/types";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return { ...actual, commands: { ...actual.commands, bootstrap: vi.fn(actual.commands.bootstrap) } };
});

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
  threads: [],
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

/** The threads the sidebar would render for the one project. */
const shown = () => appData.data?.projects[0].threads ?? [];
const titleOf = (id: string) => shown().find((thread) => thread.id === id)?.title;

describe("naming a freshly created thread", () => {
  beforeEach(() => {
    appData.data = structuredClone(data);
  });

  it("shows the opening message in place of the placeholder", () => {
    trackNewThread("thread-a", "/projects/api");
    expect(titleOf("thread-a")).toBe(UNTITLED_THREAD);

    nameNewThread("thread-a", "Fix the parser");
    expect(titleOf("thread-a")).toBe("Fix the parser");
  });

  it("does not bring back a new thread that was deleted before it persisted", () => {
    trackNewThread("thread-gone", "/projects/api");
    forgetOptimisticThread("thread-gone");

    applyData(structuredClone(data));
    expect(titleOf("thread-gone")).toBeUndefined();
  });

  it("keeps that title through refreshes that cannot see the thread yet", () => {
    trackNewThread("thread-b", "/projects/api");
    nameNewThread("thread-b", "Fix the parser");

    // Bootstrap still only knows persisted rollouts, so the thread is re-inserted.
    applyData(structuredClone(data));
    expect(titleOf("thread-b")).toBe("Fix the parser");
  });

  it("never overwrites a title that has already landed", () => {
    trackNewThread("thread-c", "/projects/api");
    nameNewThread("thread-c", "Fix the parser");
    // A later pass — the auto-namer, or a second send — must not undo it.
    nameNewThread("thread-c", "Something else");
    expect(titleOf("thread-c")).toBe("Fix the parser");
  });

  it("ignores a message with no title in it", () => {
    trackNewThread("thread-d", "/projects/api");
    nameNewThread("thread-d", "");
    expect(titleOf("thread-d")).toBe(UNTITLED_THREAD);
  });

  it("steps aside once bootstrap knows the thread", () => {
    trackNewThread("thread-e", "/projects/api");

    const caughtUp = structuredClone(data);
    caughtUp.projects[0].threads.push({
      id: "thread-e",
      cwd: "/projects/api",
      title: "Parser rewrite",
      updatedAt: 1,
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
    });
    applyData(caughtUp);

    nameNewThread("thread-e", "Fix the parser");
    expect(titleOf("thread-e")).toBe("Parser rewrite");
    expect(shown().filter((thread) => thread.id === "thread-e")).toHaveLength(1);
  });
});

describe("project placement", () => {
  it.each([
    [["/projects/batgard", "/projects/batgard-oxc"], "/projects/batgard-oxc", "/projects/batgard-oxc"],
    [["/projects/batgard-oxc", "/projects/batgard"], "/projects/batgard-oxc", "/projects/batgard-oxc"],
    [["/projects", "/projects/api"], "/projects/api/src", "/projects/api"],
    [["/projects/api"], "/projects/api-other", null],
    [["/projects/api/"], "/projects/api", "/projects/api/"],
    [["/", "/projects"], "/elsewhere", "/"],
    [["C:\\", "C:\\Projects\\api"], "C:/Projects/api/src", "C:\\Projects\\api"],
    [
      ["\\\\wsl.localhost\\Ubuntu\\repo", "\\\\wsl.localhost\\Ubuntu\\repo-api"],
      "\\\\wsl.localhost\\Ubuntu\\repo-api\\src",
      "\\\\wsl.localhost\\Ubuntu\\repo-api",
    ],
  ])("places %s by directory boundaries", (paths, cwd, expected) => {
    const snapshot = (): BootstrapData => ({
      ...structuredClone(data),
      projects: paths.map((path) => ({ ...structuredClone(api), path })),
    });
    appData.data = snapshot();
    expect(projectForCwd(cwd)?.path ?? null).toBe(expected);
    const id = `placement-${cwd}`;
    trackNewThread(id, cwd);
    nameNewThread(id, "Opening title");
    for (let i = 0; i < 2; i++) {
      const owners = appData.data!.projects.filter((project) => project.threads.some((thread) => thread.id === id));
      expect(owners.map((project) => project.path)).toEqual(expected ? [expected] : []);
      if (expected) expect(owners[0].threads.find((thread) => thread.id === id)?.title).toBe("Opening title");
      applyData(snapshot());
    }
  });

  it("uses an existing listing for a worktree outside the project", () => {
    appData.data = structuredClone(data);
    trackNewThread("worktree-fallback", api.path);
    appData.data.projects[0].threads[0].cwd = "/tmp/worktree";
    expect(projectForCwd("/tmp/worktree")?.path).toBe(api.path);
  });
});

describe("sidebar data from overlapping requests", () => {
  const titled = (title: string): BootstrapData => {
    const next = structuredClone(data);
    next.projects[0].name = title;
    return next;
  };

  it("drops a response older than one already applied", () => {
    const slowRefresh = dataTicket();
    const addProject = dataTicket();
    expect(applyTicketed(titled("after add"), addProject)).toBe(true);
    expect(applyTicketed(titled("stale refresh"), slowRefresh, false)).toBe(false);
    expect(appData.data?.projects[0].name).toBe("after add");
  });

  it("applies responses that land in order", () => {
    const first = dataTicket();
    const second = dataTicket();
    expect(applyTicketed(titled("first"), first)).toBe(true);
    expect(applyTicketed(titled("second"), second)).toBe(true);
    expect(appData.data?.projects[0].name).toBe("second");
  });
});

describe("live sidebar reads", () => {
  const titled = (title: string): BootstrapData => {
    const next = structuredClone(data);
    next.projects[0].name = title;
    return next;
  };

  it("never overlap, and requests made during one share a single read after it", async () => {
    const pending: Array<(value: BootstrapData) => void> = [];
    const bootstrap = vi.mocked(commands.bootstrap);
    bootstrap.mockReset();
    bootstrap.mockImplementation(() => new Promise((resolve) => pending.push(resolve)));

    // Opening a Home: the initial refresh, then the `account/updated` the
    // app-server sends in reply to it, and a thread event, while it runs.
    const opening = refresh();
    const accountUpdated = quietRefresh();
    const threadStarted = quietRefresh();
    expect(bootstrap).toHaveBeenCalledTimes(1);

    pending[0](titled("first read"));
    await vi.waitFor(() => expect(bootstrap).toHaveBeenCalledTimes(2));
    pending[1](titled("read after the events"));
    await Promise.all([opening, accountUpdated, threadStarted]);

    expect(bootstrap).toHaveBeenCalledTimes(2);
    expect(appData.data?.projects[0].name).toBe("read after the events");
  });
});
