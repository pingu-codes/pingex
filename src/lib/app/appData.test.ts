import { beforeEach, describe, expect, it } from "vitest";
import { appData, applyData, nameNewThread, trackNewThread, UNTITLED_THREAD } from "$lib/app/appData.svelte";
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
    });
    applyData(caughtUp);

    nameNewThread("thread-e", "Fix the parser");
    expect(titleOf("thread-e")).toBe("Parser rewrite");
    expect(shown().filter((thread) => thread.id === "thread-e")).toHaveLength(1);
  });
});
