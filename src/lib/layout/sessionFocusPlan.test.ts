import { beforeEach, describe, expect, it } from "vitest";
import { resetTouched, touchThread } from "$lib/layout/sessionFocus.svelte";
import { isEmptyPlan, sessionFocusPlan } from "$lib/layout/sessionFocusPlan";
import type { Project, SidebarFolder, SidebarLayout, ThreadSummary } from "$lib/types";

const thread = (id: string, extra: Partial<ThreadSummary> = {}): ThreadSummary => ({
  id,
  cwd: "",
  title: id,
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
  ...extra,
});

const project = (path: string, threads: ThreadSummary[], extra: Partial<Project> = {}): Project => ({
  name: path,
  path,
  kind: "folder",
  workspaceId: null,
  pinned: false,
  archived: false,
  expanded: true,
  instructions: "",
  sources: [],
  threads,
  ...extra,
});

const folder = (id: string, scope: string, expanded = true): SidebarFolder => ({
  id,
  scope,
  parentId: null,
  name: id,
  expanded,
  ordinal: 0,
});

describe("sessionFocusPlan", () => {
  beforeEach(() => resetTouched());

  it("hides untouched, unfavorited threads and keeps the selected one", () => {
    touchThread("opened");
    const plan = sessionFocusPlan(
      [
        project("/a", [
          thread("opened"),
          thread("fav", { pinned: true }),
          thread("current"),
          thread("stale"),
          thread("already", { hidden: true }),
        ]),
      ],
      { folders: [], placements: [] },
      "current",
    );
    expect(plan).toEqual({ hide: ["stale"], collapseProjects: [], collapseFolders: [] });
  });

  it("collapses projects and folders left without a visible thread", () => {
    touchThread("a1");
    const layout: SidebarLayout = {
      folders: [folder("quiet", "/a"), folder("live", "/a"), folder("closed", "/a", false), folder("root-quiet", "")],
      placements: [
        { itemKey: "a1", scope: "/a", parentId: "live", ordinal: 0 },
        { itemKey: "a2", scope: "/a", parentId: "quiet", ordinal: 0 },
        { itemKey: "a3", scope: "/a", parentId: "closed", ordinal: 0 },
        { itemKey: "/b", scope: "", parentId: "root-quiet", ordinal: 0 },
      ],
    };
    const plan = sessionFocusPlan(
      [
        project("/a", [thread("a1"), thread("a2"), thread("a3")]),
        project("/b", [thread("b1")]),
        project("/c", [thread("c1")], { expanded: false }),
        project("/d", [thread("d1")], { archived: true }),
      ],
      layout,
      null,
    );
    expect(plan.hide).toEqual(["a2", "a3", "b1", "c1"]);
    expect(plan.collapseProjects).toEqual(["/b"]);
    // `closed` is already collapsed; `root-quiet` only holds the emptied /b.
    expect(plan.collapseFolders).toEqual(["quiet", "root-quiet"]);
  });

  it("is empty when everything visible was touched", () => {
    touchThread("a1");
    const plan = sessionFocusPlan([project("/a", [thread("a1")])], { folders: [], placements: [] }, null);
    expect(isEmptyPlan(plan)).toBe(true);
  });
});
