import { describe, expect, it } from "vitest";
import {
  buildTree,
  childrenOf,
  deleteFromLayout,
  hoistActive,
  isFolderOrDescendant,
  isNoopDrop,
  placeInLayout,
  pruneEmptyFolders,
  resolveDrop,
  siblingsAfterDrop,
} from "$lib/layout/sidebarTree";
import type { SidebarFolder, SidebarLayout } from "$lib/types";

type Item = { key: string; pinned?: boolean };
const adapter = { key: (item: Item) => item.key, pinned: (item: Item) => item.pinned ?? false };

const folder = (id: string, parentId: string | null = null, ordinal = 0, scope = ""): SidebarFolder => ({
  id,
  scope,
  parentId,
  name: id,
  expanded: true,
  ordinal,
});

const ids = (nodes: ReturnType<typeof buildTree<Item>>) => nodes.map((node) => `${node.kind}:${node.id}`);

describe("buildTree", () => {
  it("keeps never-dragged items in backend order at the root", () => {
    const tree = buildTree({ folders: [], placements: [] }, "", [{ key: "a" }, { key: "b" }], adapter);
    expect(ids(tree)).toEqual(["item:a", "item:b"]);
  });

  // Unreleased Codex reports when a project was last active; only never-dragged
  // items use it, so a deliberate arrangement is never reshuffled.
  it("orders unplaced items by recency when the adapter knows it", () => {
    type Recent = Item & { recency?: number | null };
    const recent = { ...adapter, recency: (item: Recent) => item.recency };
    const items: Recent[] = [{ key: "old", recency: 10 }, { key: "unknown" }, { key: "new", recency: 20 }];
    expect(ids(buildTree({ folders: [], placements: [] }, "", items, recent))).toEqual([
      "item:new",
      "item:old",
      "item:unknown",
    ]);
    const layout: SidebarLayout = {
      folders: [],
      placements: [{ itemKey: "old", scope: "", parentId: null, ordinal: 0 }],
    };
    expect(ids(buildTree(layout, "", items, recent))).toEqual(["item:old", "item:new", "item:unknown"]);
  });

  it("orders pinned, then placed by ordinal, then unplaced", () => {
    const layout: SidebarLayout = {
      folders: [folder("f", null, 1)],
      placements: [
        { itemKey: "late", scope: "", parentId: null, ordinal: 5 },
        { itemKey: "early", scope: "", parentId: null, ordinal: 0 },
      ],
    };
    const items: Item[] = [{ key: "late" }, { key: "fresh" }, { key: "early" }, { key: "pin", pinned: true }];
    expect(ids(buildTree(layout, "", items, adapter))).toEqual([
      "item:pin",
      "item:early",
      "folder:f",
      "item:late",
      "item:fresh",
    ]);
  });

  it("nests folders and items and lifts orphans to the root", () => {
    const layout: SidebarLayout = {
      folders: [folder("outer"), folder("inner", "outer"), folder("lost", "missing")],
      placements: [
        { itemKey: "a", scope: "", parentId: "inner", ordinal: 0 },
        { itemKey: "b", scope: "", parentId: "missing", ordinal: 0 },
        { itemKey: "c", scope: "/other", parentId: "outer", ordinal: 0 },
      ],
    };
    const tree = buildTree(layout, "", [{ key: "a" }, { key: "b" }, { key: "c" }], adapter);
    expect(ids(tree)).toEqual(["folder:outer", "folder:lost", "item:b", "item:c"]);
    expect(ids(childrenOf(tree, "outer") ?? [])).toEqual(["folder:inner"]);
    expect(ids(childrenOf(tree, "inner") ?? [])).toEqual(["item:a"]);
  });

  it("only sees folders of its own scope", () => {
    const layout: SidebarLayout = { folders: [folder("p", null, 0, "/proj")], placements: [] };
    expect(buildTree(layout, "", [], adapter)).toEqual([]);
    expect(ids(buildTree(layout, "/proj", [], adapter))).toEqual(["folder:p"]);
  });
});

describe("resolveDrop", () => {
  const layout: SidebarLayout = {
    folders: [folder("f", null, 0), folder("g", "f", 0)],
    placements: [{ itemKey: "in", scope: "", parentId: "f", ordinal: 1 }],
  };
  const tree = buildTree(layout, "", [{ key: "a" }, { key: "b" }, { key: "in" }], adapter);
  const rect = { top: 100, height: 30 };

  it("maps thirds to before / inside / after on a folder", () => {
    const drag = { kind: "item" as const, id: "a" };
    const row = { kind: "folder" as const, id: "f" };
    expect(resolveDrop(tree, layout, drag, row, 102, rect)).toMatchObject({
      zone: "before",
      parentId: null,
      before: row,
    });
    expect(resolveDrop(tree, layout, drag, row, 115, rect)).toMatchObject({
      zone: "inside",
      parentId: "f",
      before: null,
    });
    expect(resolveDrop(tree, layout, drag, row, 128, rect)).toMatchObject({
      zone: "after",
      parentId: null,
      before: { kind: "item", id: "a" },
    });
  });

  it("snaps the middle of a plain item to an edge", () => {
    const drag = { kind: "item" as const, id: "b" };
    expect(resolveDrop(tree, layout, drag, { kind: "item", id: "a" }, 114, rect)).toMatchObject({ zone: "before" });
    expect(resolveDrop(tree, layout, drag, { kind: "item", id: "a" }, 116, rect)).toMatchObject({
      zone: "after",
      before: { kind: "item", id: "b" },
    });
  });

  it("refuses dropping onto itself or into its own subtree", () => {
    expect(resolveDrop(tree, layout, { kind: "item", id: "a" }, { kind: "item", id: "a" }, 110, rect)).toBeNull();
    expect(resolveDrop(tree, layout, { kind: "folder", id: "f" }, { kind: "folder", id: "g" }, 115, rect)).toBeNull();
    expect(isFolderOrDescendant(layout.folders, "f", "g")).toBe(true);
    expect(isFolderOrDescendant(layout.folders, "g", "f")).toBe(false);
  });

  it("computes the resulting sibling order and detects no-ops", () => {
    const item = { kind: "item" as const, id: "b" };
    const target = { parentId: null, before: { kind: "item" as const, id: "a" }, rowId: "", zone: "before" as const };
    expect(siblingsAfterDrop(tree, item, target)).toEqual([
      { kind: "folder", id: "f" },
      { kind: "item", id: "b" },
      { kind: "item", id: "a" },
    ]);
    expect(isNoopDrop(tree, item, target)).toBe(false);
    expect(isNoopDrop(tree, item, { parentId: null, before: null, rowId: "", zone: "after" })).toBe(true);
    expect(siblingsAfterDrop(tree, item, { parentId: "f", before: null, rowId: "", zone: "inside" })).toEqual([
      { kind: "folder", id: "g" },
      { kind: "item", id: "in" },
      { kind: "item", id: "b" },
    ]);
  });
});

describe("layout mutations", () => {
  it("places an item and renumbers the destination siblings", () => {
    const layout: SidebarLayout = { folders: [folder("f")], placements: [] };
    const next = placeInLayout(layout, "", { kind: "item", id: "b" }, null, [
      { kind: "item", id: "b" },
      { kind: "folder", id: "f" },
      { kind: "item", id: "a" },
    ]);
    expect(next.folders[0].ordinal).toBe(1);
    expect(next.placements).toEqual([
      { itemKey: "b", scope: "", parentId: null, ordinal: 0 },
      { itemKey: "a", scope: "", parentId: null, ordinal: 2 },
    ]);
    const moved = placeInLayout(next, "", { kind: "item", id: "b" }, "f", [{ kind: "item", id: "b" }]);
    expect(moved.placements.find((p) => p.itemKey === "b")).toMatchObject({ parentId: "f", ordinal: 0 });
  });

  it("deleting a folder lifts its contents to its parent", () => {
    const layout: SidebarLayout = {
      folders: [folder("outer"), folder("inner", "outer")],
      placements: [{ itemKey: "a", scope: "", parentId: "inner", ordinal: 0 }],
    };
    const next = deleteFromLayout(layout, "inner");
    expect(next.folders.map((f) => f.id)).toEqual(["outer"]);
    expect(next.placements[0].parentId).toBe("outer");
  });
});

describe("session focus helpers", () => {
  const layout: SidebarLayout = {
    folders: [folder("empty", null, 0), folder("busy", null, 1), folder("nested", "busy", 0)],
    placements: [
      { itemKey: "in-busy", scope: "", parentId: "busy", ordinal: 0 },
      { itemKey: "deep", scope: "", parentId: "nested", ordinal: 0 },
    ],
  };
  const items: Item[] = [{ key: "a" }, { key: "b", pinned: true }, { key: "in-busy" }, { key: "deep" }];

  it("pruneEmptyFolders drops folders with no items, recursively", () => {
    const tree = pruneEmptyFolders(buildTree(layout, "", items, adapter));
    expect(ids(tree)).toEqual(["item:b", "folder:busy", "item:a"]);
    const busy = tree[1];
    expect(busy.kind === "folder" && ids(busy.children)).toEqual(["folder:nested", "item:in-busy"]);
    expect(pruneEmptyFolders(buildTree(layout, "", [], adapter))).toEqual([]);
  });

  it("hoistActive lifts active subtrees without reordering within a group", () => {
    const tree = buildTree(layout, "", items, adapter);
    expect(ids(tree)).toEqual(["item:b", "folder:empty", "folder:busy", "item:a"]);
    const active = new Set(["a", "deep"]);
    const hoisted = hoistActive(tree, (item) => active.has(item.key));
    expect(ids(hoisted)).toEqual(["folder:busy", "item:a", "item:b", "folder:empty"]);
    const busy = hoisted[0];
    expect(busy.kind === "folder" && ids(busy.children)).toEqual(["folder:nested", "item:in-busy"]);
    // Input is left untouched.
    expect(ids(tree)).toEqual(["item:b", "folder:empty", "folder:busy", "item:a"]);
  });
});
