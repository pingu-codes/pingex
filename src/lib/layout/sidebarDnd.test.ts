import { describe, expect, it, vi } from "vitest";
import { dnd, rowRef, startDrag } from "$lib/layout/sidebarDnd.svelte";

const pointer = (type: string, x: number, y: number) =>
  Object.assign(new MouseEvent(type, { clientX: x, clientY: y, button: 0, bubbles: true }), {}) as PointerEvent;

const source = { scope: "", ref: { kind: "item" as const, id: "/a" }, label: "a" };

describe("startDrag", () => {
  it("ignores presses that never travel past the threshold", () => {
    const commit = vi.fn();
    startDrag(pointer("pointerdown", 10, 10), source, { resolve: () => null, commit });
    window.dispatchEvent(pointer("pointermove", 12, 11));
    expect(dnd.dragging).toBeNull();
    window.dispatchEvent(pointer("pointerup", 12, 11));
    expect(commit).not.toHaveBeenCalled();
    expect(dnd.suppressClick).toBe(false);
  });

  it("tracks the pointer, commits the resolved target on release and suppresses the click", () => {
    const row = document.createElement("div");
    row.dataset.sidebarRow = "folder:f";
    row.dataset.sidebarScope = "";
    document.body.append(row);
    document.elementFromPoint = () => row;
    const target = { parentId: "f", before: null, rowId: "folder:f", zone: "inside" as const };
    const commit = vi.fn();
    startDrag(pointer("pointerdown", 10, 10), source, { resolve: () => target, commit });
    window.dispatchEvent(pointer("pointermove", 40, 40));
    expect(dnd.dragging).toEqual(source);
    expect(dnd.over).toEqual(target);
    expect([dnd.x, dnd.y]).toEqual([40, 40]);
    window.dispatchEvent(pointer("pointerup", 40, 40));
    expect(commit).toHaveBeenCalledWith(target);
    expect(dnd.dragging).toBeNull();
    expect(dnd.suppressClick).toBe(true);
    row.remove();
  });

  it("escape cancels without committing", () => {
    const commit = vi.fn();
    startDrag(pointer("pointerdown", 0, 0), source, { resolve: () => null, commit });
    window.dispatchEvent(pointer("pointermove", 50, 50));
    expect(dnd.dragging).toEqual(source);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(dnd.dragging).toBeNull();
    window.dispatchEvent(pointer("pointerup", 50, 50));
    expect(commit).not.toHaveBeenCalled();
  });
});

describe("rowRef", () => {
  it("parses the row dataset, keeping colons inside ids", () => {
    const element = document.createElement("div");
    element.dataset.sidebarRow = "item:/Users/x:y";
    element.dataset.sidebarScope = "/p";
    expect(rowRef(element)).toEqual({ scope: "/p", ref: { kind: "item", id: "/Users/x:y" } });
    element.dataset.sidebarRow = "nope:1";
    expect(rowRef(element)).toBeNull();
  });
});
