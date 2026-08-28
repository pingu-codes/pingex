import { afterEach, describe, expect, it, vi } from "vitest";
import type { ComposerPart } from "$lib/composer/composerParts";
import { RichEditor, type RichEditorDeps } from "$lib/composer/richEditor.svelte";
import { chipBesideCaret, readParts } from "$lib/composer/richInput";
import { COALESCE_MS } from "$lib/composer/undoStack";

const mention: ComposerPart = { type: "mention", name: "utils.ts", path: "/proj/src/lib/utils.ts" };
const text = (t: string): ComposerPart => ({ type: "text", text: t });

interface Harness {
  editor: RichEditor;
  root: HTMLElement;
  deps: RichEditorDeps;
  clock: { now: number };
}

/** A RichEditor bound to a real element in the document, holding `parts`
 *  with the caret at `caret` (default: the end) and an empty undo history. */
function mount(parts: ComposerPart[], caret: number | null = null, over: Partial<RichEditorDeps> = {}): Harness {
  const root = document.createElement("div");
  root.contentEditable = "true";
  document.body.append(root);
  const clock = { now: 1000 };
  const deps: RichEditorDeps = {
    cwd: () => "/proj",
    chipHandlers: { onRetry: () => {} },
    pickerActive: () => false,
    pickerOpen: () => false,
    onSubmit: vi.fn(),
    onHistory: vi.fn(() => false),
    onChipRemoved: vi.fn(),
    onEdit: vi.fn(),
    now: () => clock.now,
    ...over,
  };
  const editor = new RichEditor(deps);
  editor.attach(root);
  root.addEventListener("keydown", editor.handleKey);
  root.addEventListener("input", editor.handleInput);
  editor.setParts(parts, caret);
  editor.resetUndo();
  return { editor, root, deps, clock };
}

interface Mods {
  meta?: boolean;
  alt?: boolean;
  ctrl?: boolean;
  shift?: boolean;
}

/** Dispatch a keydown on the editor element, as the browser would. */
function press(root: HTMLElement, key: string, mods: Mods = {}): KeyboardEvent {
  const event = new KeyboardEvent("keydown", {
    key,
    cancelable: true,
    bubbles: true,
    metaKey: mods.meta ?? false,
    altKey: mods.alt ?? false,
    ctrlKey: mods.ctrl ?? false,
    shiftKey: mods.shift ?? false,
  });
  root.dispatchEvent(event);
  return event;
}

/** Simulate the browser typing `typed` at the caret, then firing `input`. */
function type(root: HTMLElement, typed: string) {
  const selection = window.getSelection();
  const range = selection?.getRangeAt(0);
  if (!range) throw new Error("no caret to type at");
  range.insertNode(document.createTextNode(typed));
  range.collapse(false);
  selection?.removeAllRanges();
  selection?.addRange(range);
  root.normalize();
  root.dispatchEvent(new Event("input", { bubbles: true }));
}

function liveRange(): Range {
  return window.getSelection()?.getRangeAt(0) as Range;
}

afterEach(() => {
  document.body.replaceChildren();
  window.getSelection()?.removeAllRanges();
});

describe("RichEditor line breaks", () => {
  it("Shift+Enter before a chip adds one break and leaves the caret beside the chip", () => {
    // Units: "look at " = 0-8, chip = 8-9. Caret at 8 is right before the chip.
    const { editor, root } = mount([text("look at "), mention], 8);

    const event = press(root, "Enter", { shift: true });

    expect(event.defaultPrevented).toBe(true);
    expect(editor.parts).toEqual([text("look at \n"), mention]);
    // One break, not the two WebKit writes beside a contenteditable=false chip.
    expect(root.querySelectorAll("br")).toHaveLength(1);
    expect(chipBesideCaret("forward", liveRange())).toBe(root.querySelector("[data-mention-path]"));
  });

  it("Enter without Shift submits instead of breaking the line", () => {
    const { editor, root, deps } = mount([text("send me")]);

    const event = press(root, "Enter");

    expect(event.defaultPrevented).toBe(true);
    expect(deps.onSubmit).toHaveBeenCalledOnce();
    expect(editor.parts).toEqual([text("send me")]);
  });

  it("Backspace at the start of a line removes exactly one break", () => {
    const { editor, root } = mount([text("one\n\ntwo")], 5);

    const event = press(root, "Backspace");

    expect(event.defaultPrevented).toBe(true);
    expect(editor.parts).toEqual([text("one\ntwo")]);
    expect(editor.caretOffset()).toBe(4);
  });

  it("Delete ahead of a break removes it", () => {
    const { editor, root } = mount([text("one\ntwo")], 3);

    press(root, "Delete");

    expect(editor.parts).toEqual([text("onetwo")]);
    expect(editor.caretOffset()).toBe(3);
  });

  it("leaves an ordinary character delete to the browser", () => {
    const { editor, root } = mount([text("one\ntwo")], 2);

    const event = press(root, "Backspace");

    expect(event.defaultPrevented).toBe(false);
    expect(editor.parts).toEqual([text("one\ntwo")]);
  });
});

describe("RichEditor chips", () => {
  it("Backspace beside a chip removes it and tells the owner", () => {
    // "see " = 0-4, chip = 4-5, " now" = 5-9.
    const { editor, root, deps } = mount([text("see "), mention, text(" now")], 5);
    const chip = root.querySelector("[data-mention-path]");

    const event = press(root, "Backspace");

    expect(event.defaultPrevented).toBe(true);
    expect(deps.onChipRemoved).toHaveBeenCalledWith(chip);
    expect(editor.parts).toEqual([text("see  now")]);
    expect(editor.caretOffset()).toBe(4);
  });

  it("Cmd+Backspace deletes the whole line, chip included, not just the chip", () => {
    const { editor, root, deps } = mount([text("one\nsee "), mention, text(" now")], 13);

    const event = press(root, "Backspace", { meta: true });

    expect(event.defaultPrevented).toBe(true);
    expect(editor.parts).toEqual([text("one\n")]);
    expect(deps.onChipRemoved).not.toHaveBeenCalled();
  });

  it("Option+Backspace deletes back through a chip as one word", () => {
    const { editor, root } = mount([text("see "), mention, text(" now")], 6);

    const event = press(root, "Backspace", { alt: true });

    expect(event.defaultPrevented).toBe(true);
    expect(editor.parts.some((part) => part.type === "mention")).toBe(false);
    expect(editor.parts).toEqual([text("see now")]);
  });

  it("Option+Backspace inside plain text is left to the browser", () => {
    const { root } = mount([text("plain words")], 11);

    const event = press(root, "Backspace", { alt: true });

    expect(event.defaultPrevented).toBe(false);
  });

  it("ArrowLeft/ArrowRight step over a chip instead of stalling on it", () => {
    const { editor, root } = mount([text("see "), mention, text(" now")], 5);

    expect(press(root, "ArrowLeft").defaultPrevented).toBe(true);
    expect(editor.caretOffset()).toBe(4);

    expect(press(root, "ArrowRight").defaultPrevented).toBe(true);
    expect(editor.caretOffset()).toBe(5);
  });

  it("ArrowRight crosses a line break onto a chip that starts the next line", () => {
    const { editor, root } = mount([text("one\n"), mention, text(" two")], 3);

    expect(press(root, "ArrowRight").defaultPrevented).toBe(true);
    expect(editor.caretOffset()).toBe(4);
  });

  it("Cmd+ArrowRight reaches the end of a line that ends in a chip", () => {
    const { editor, root } = mount([text("see "), mention], 0);

    expect(press(root, "ArrowRight", { meta: true }).defaultPrevented).toBe(true);
    expect(editor.caretOffset()).toBe(5);
  });

  it("Option+ArrowRight treats a chip as one word", () => {
    const { editor, root } = mount([text("see "), mention, text(" now")], 0);

    press(root, "ArrowRight", { alt: true });
    expect(editor.caretOffset()).toBe(3);
    press(root, "ArrowRight", { alt: true });
    expect(editor.caretOffset()).toBe(5);
  });

  it("inserts a mention chip in place of the open @query", () => {
    const { editor, root } = mount([text("see @uti")]);
    expect(editor.queries.mentionQuery).toBe("uti");

    expect(editor.insertMention({ name: "utils.ts", path: "/proj/src/lib/utils.ts" })).toBe(true);

    expect(editor.parts).toEqual([text("see "), mention]);
    expect(editor.queries.mentionQuery).toBeNull();
    expect(root.querySelector("[data-mention-path]")).not.toBeNull();
  });

  it("dismisses a stale mention query rather than inserting into the wrong place", () => {
    const { editor } = mount([text("see @uti")]);
    editor.setText("replaced");

    expect(editor.insertMention({ name: "utils.ts", path: "/proj/src/lib/utils.ts" })).toBe(false);
    expect(editor.queries.mentionQuery).toBeNull();
    expect(editor.parts).toEqual([text("replaced")]);
  });
});

describe("RichEditor queries", () => {
  it("re-detects after an edit that bypassed the browser, so no picker goes stale", () => {
    const { editor, root } = mount([text("see @uti")]);
    expect(editor.queries.mentionQuery).toBe("uti");

    press(root, "Backspace", { meta: true });

    expect(editor.parts).toEqual([text("")]);
    expect(editor.queries.mentionQuery).toBeNull();
    expect(editor.queries.mentionRange).toBeNull();
  });

  it("does not detect while the owner suppresses queries", () => {
    const { editor, root } = mount([text("")], null, { suppressQueries: () => true });

    type(root, "/rev");

    expect(editor.parts).toEqual([text("/rev")]);
    expect(editor.queries.slashQuery).toBeNull();
  });

  it("closeQueries forgets the pickers without touching the text", () => {
    const { editor } = mount([text("see @uti")]);

    editor.closeQueries();

    expect(editor.queries.mentionQuery).toBeNull();
    expect(editor.parts).toEqual([text("see @uti")]);
  });
});

describe("RichEditor input", () => {
  it("flattens the browser's block line and restores chip padding", () => {
    // WebKit's answer to a newline typed before a chip: the chip ends up in a
    // <div> of its own, where it is no longer a sibling of the caret.
    const { editor, root } = mount([text("one"), mention]);
    const chip = root.querySelector("[data-mention-path]") as HTMLElement;
    const block = document.createElement("div");
    root.append(block);
    block.append(chip);
    for (const node of [...root.childNodes]) if (node.nodeType === Node.TEXT_NODE && !node.textContent) node.remove();
    const selection = window.getSelection();
    const range = document.createRange();
    range.setStart(block, 1);
    range.collapse(true);
    selection?.removeAllRanges();
    selection?.addRange(range);

    root.dispatchEvent(new Event("input", { bubbles: true }));

    expect(editor.parts).toEqual([text("one\n"), mention]);
    expect(root.querySelector("div")).toBeNull();
    expect(chipBesideCaret("back", liveRange())).toBe(root.querySelector("[data-mention-path]"));
  });

  it("ignores input events mid-composition and reads once it ends", () => {
    const { editor, root } = mount([text("")], 0);

    editor.handleCompositionStart();
    type(root, "日本");
    expect(editor.parts).toEqual([text("")]);

    editor.handleCompositionEnd();
    expect(editor.parts).toEqual([text("日本")]);
  });

  it("reports every content change to the owner, restores included", () => {
    const { editor, root, deps } = mount([text("")], 0);
    vi.mocked(deps.onEdit as () => void).mockClear();

    type(root, "a");
    expect(deps.onEdit).toHaveBeenCalledTimes(1);

    editor.undo();
    expect(deps.onEdit).toHaveBeenCalledTimes(2);
  });
});

describe("RichEditor undo", () => {
  it("coalesces a burst of typing and splits on the clock", () => {
    const { editor, root, clock } = mount([text("")], 0);

    type(root, "hel");
    clock.now += 50;
    type(root, "lo");
    clock.now += COALESCE_MS + 1;
    type(root, " world");
    expect(editor.parts).toEqual([text("hello world")]);

    press(root, "z", { meta: true });
    expect(editor.parts).toEqual([text("hello")]);
    expect(editor.caretOffset()).toBe(5);

    press(root, "z", { meta: true });
    expect(editor.parts).toEqual([text("")]);

    press(root, "z", { meta: true, shift: true });
    expect(editor.parts).toEqual([text("hello")]);

    press(root, "y", { ctrl: true });
    expect(editor.parts).toEqual([text("hello world")]);
  });

  it("gives a structural edit its own entry even inside the coalesce window", () => {
    const { editor, root } = mount([text("")], 0);

    type(root, "one");
    press(root, "Enter", { shift: true });
    type(root, "two");
    expect(editor.parts).toEqual([text("one\ntwo")]);

    editor.undo();
    expect(editor.parts).toEqual([text("one\n")]);
    editor.undo();
    expect(editor.parts).toEqual([text("one")]);
  });

  it("undoes a chip removal", () => {
    const { editor, root } = mount([text("see "), mention, text(" now")], 5);

    press(root, "Backspace");
    expect(editor.parts).toEqual([text("see  now")]);

    editor.undo();
    expect(editor.parts).toEqual([text("see "), mention, text(" now")]);
    expect(editor.caretOffset()).toBe(5);
  });

  it("clear() empties the editor and cannot be undone past", () => {
    const { editor, root } = mount([text("sent")]);

    editor.clear();

    expect(editor.parts).toEqual([text("")]);
    expect(editor.canUndo).toBe(false);
    expect(press(root, "z", { meta: true }).defaultPrevented).toBe(true);
    expect(editor.parts).toEqual([text("")]);
  });

  it("setParts puts the caret at the end and records an entry", () => {
    const { editor } = mount([text("")], 0);

    editor.setParts([text("one\ntwo")]);

    expect(editor.caretOffset()).toBe(7);
    expect(editor.canUndo).toBe(true);
    editor.undo();
    expect(editor.parts).toEqual([text("")]);
  });
});

describe("RichEditor key ownership", () => {
  it("lets an active picker own Enter and the arrows", () => {
    const { root, deps } = mount([text("/co")], null, { pickerActive: () => true, pickerOpen: () => true });

    expect(press(root, "Enter").defaultPrevented).toBe(true);
    expect(deps.onSubmit).not.toHaveBeenCalled();
    expect(press(root, "ArrowDown").defaultPrevented).toBe(true);
    expect(press(root, "Escape").defaultPrevented).toBe(true);
  });

  it("asks history before moving the caret on a bare ArrowUp", () => {
    const onHistory = vi.fn(() => true);
    const { root, deps } = mount([text("one\ntwo")], 5, { onHistory });

    expect(press(root, "ArrowUp").defaultPrevented).toBe(true);
    expect(onHistory).toHaveBeenCalledWith("older");
    expect(deps.onHistory).toBe(onHistory);
  });

  it("leaves vertical motion to the browser when history declines", () => {
    // jsdom has no `Selection.modify`, so the by-hand fallback (which only
    // runs once native motion is seen to stall) is unreachable here.
    const { editor, root } = mount([text("one\ntwo")], 6);

    expect(press(root, "ArrowUp").defaultPrevented).toBe(false);
    expect(editor.caretOffset()).toBe(6);
  });

  it("caretOnEdgeLine reads the caret against the parts", () => {
    const { editor } = mount([text("one\ntwo")], 2);

    expect(editor.caretOnEdgeLine("first")).toBe(true);
    expect(editor.caretOnEdgeLine("last")).toBe(false);
  });

  it("stays inert once detached", () => {
    const { editor, root } = mount([text("one\ntwo")], 5);
    editor.attach(root)();

    expect(press(root, "Backspace").defaultPrevented).toBe(false);
    expect(editor.caretOffset()).toBeNull();
  });
});

describe("RichEditor attachments", () => {
  const staging: ComposerPart = {
    type: "attachment",
    id: "c0",
    filename: "diagram.png",
    mime: "image/png",
    size: 10,
    path: "",
    kind: "image",
    state: "staging",
  };

  it("inserts at the caret and updates the chip in place", () => {
    const { editor, root } = mount([text("see  now")], 4);

    editor.insertAttachment(staging);
    expect(editor.parts).toEqual([text("see "), staging, text(" now")]);

    editor.updateAttachment("c0", { ...staging, id: "a1", path: "/staged/a1.png", state: "ready" });
    expect(editor.parts).toEqual([
      text("see "),
      { ...staging, id: "a1", path: "/staged/a1.png", state: "ready" },
      text(" now"),
    ]);
    expect(root.querySelector("[data-attachment-id='a1']")).not.toBeNull();
  });

  it("appends when the caret is outside the editor", () => {
    const { editor } = mount([text("see")]);
    window.getSelection()?.removeAllRanges();

    editor.insertAttachment(staging);

    expect(editor.parts).toEqual([text("see"), staging]);
    expect(readParts(editor.element as HTMLElement)).toEqual(editor.parts);
  });
});
