import { afterEach, describe, expect, it } from "vitest";
import {
  type AttachmentPart,
  buildTurnInput,
  type ComposerPart,
  chipBesideCaret,
  deleteLineBreak,
  deleteToLineEdge,
  detectQueries,
  formatSize,
  hasSendableContent,
  insertAttachmentChip,
  insertLineBreak,
  moveCaretToLineEdge,
  normaliseEditorDom,
  placeCaretBesideChip,
  readParts,
  renderParts,
  renderPartsWith,
  updateAttachmentChip,
} from "$lib/composer/richInput";

function chipNode(name = "utils.ts"): HTMLElement {
  const chip = document.createElement("span");
  chip.contentEditable = "false";
  chip.dataset.mentionPath = `/proj/src/lib/${name}`;
  chip.dataset.mentionName = name;
  chip.textContent = `@${name}`;
  return chip;
}

function editorWith(...nodes: (Node | string)[]): HTMLElement {
  const editor = document.createElement("div");
  editor.append(...nodes);
  document.body.append(editor);
  return editor;
}

function caret(node: Node, offset: number): Range {
  const range = document.createRange();
  range.setStart(node, offset);
  range.collapse(true);
  return range;
}

afterEach(() => {
  document.body.replaceChildren();
  window.getSelection()?.removeAllRanges();
});

describe("chipBesideCaret", () => {
  it("finds the chip ahead of a caret at the end of the preceding text", () => {
    const chip = chipNode();
    const text = document.createTextNode("see ");
    editorWith(text, chip);

    expect(chipBesideCaret("forward", caret(text, 4))).toBe(chip);
    expect(chipBesideCaret("forward", caret(text, 2))).toBeNull();
  });

  it("finds the chip behind a caret at the start of the following text", () => {
    const chip = chipNode();
    const text = document.createTextNode(" explain");
    editorWith(chip, text);

    expect(chipBesideCaret("back", caret(text, 0))).toBe(chip);
    expect(chipBesideCaret("back", caret(text, 3))).toBeNull();
  });

  it("skips the empty text node inserted after a chip", () => {
    const chip = chipNode();
    const tail = document.createTextNode("");
    const text = document.createTextNode("explain");
    editorWith(chip, tail, text);

    expect(chipBesideCaret("back", caret(text, 0))).toBe(chip);
  });

  it("resolves element-level carets, skipping empty padding", () => {
    const chip = chipNode();
    const tail = document.createTextNode("");
    const editor = editorWith(document.createTextNode("see "), chip, tail);

    // Caret expressed as (editor, index) rather than inside a text node.
    expect(chipBesideCaret("back", caret(editor, 2))).toBe(chip);
    expect(chipBesideCaret("back", caret(editor, 1))).toBeNull();
    expect(chipBesideCaret("forward", caret(editor, 1))).toBe(chip);
    expect(chipBesideCaret("forward", caret(editor, 2))).toBeNull();
  });

  it("ignores non-collapsed selections", () => {
    const chip = chipNode();
    const text = document.createTextNode("see ");
    editorWith(text, chip);
    const range = document.createRange();
    range.setStart(text, 0);
    range.setEnd(text, 4);

    expect(chipBesideCaret("forward", range)).toBeNull();
  });
});

describe("placeCaretBesideChip", () => {
  it("collapses the selection to either side of the chip", () => {
    const chip = chipNode();
    const editor = editorWith(document.createTextNode("see "), chip, document.createTextNode(" now"));

    placeCaretBesideChip(chip, "after");
    let range = window.getSelection()?.getRangeAt(0);
    expect(range?.startContainer).toBe(editor);
    expect(range?.startOffset).toBe(2);
    expect(range?.collapsed).toBe(true);

    placeCaretBesideChip(chip, "before");
    range = window.getSelection()?.getRangeAt(0);
    expect(range?.startOffset).toBe(1);
  });
});

describe("moveCaretToLineEdge", () => {
  it("moves the caret past a trailing chip to the editor edge", () => {
    const text = document.createTextNode("see ");
    const chip = chipNode();
    const editor = editorWith(text, chip, document.createTextNode(""));
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(text, 0));

    moveCaretToLineEdge(editor, "forward");
    const range = selection?.getRangeAt(0);
    expect(range?.collapsed).toBe(true);
    expect(range?.startContainer).toBe(editor);
    expect(range?.startOffset).toBe(editor.childNodes.length);
  });

  it("does nothing when the selection is outside the editor", () => {
    const editor = editorWith(document.createTextNode("inside"));
    const outside = document.createTextNode("outside");
    document.body.append(outside);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(outside, 3));

    moveCaretToLineEdge(editor, "forward");
    const range = selection?.getRangeAt(0);
    expect(range?.startContainer).toBe(outside);
    expect(range?.startOffset).toBe(3);
  });
});

describe("readParts line breaks", () => {
  const textOf = (editor: HTMLElement) =>
    readParts(editor)
      .map((part) => ("text" in part ? part.text : ""))
      .join("");
  const fromHtml = (html: string) => {
    const editor = editorWith();
    editor.innerHTML = html;
    return editor;
  };

  it("puts a browser block line's break before it, not after", () => {
    // WebKit answers Shift+Enter with a <div> wrapper for the new line.
    expect(textOf(fromHtml("one<div>two</div>"))).toBe("one\ntwo");
    expect(textOf(fromHtml("<div>one</div><div>two</div>"))).toBe("one\ntwo");
    expect(textOf(fromHtml("<div>one</div>two"))).toBe("one\ntwo");
  });

  it("counts a blank browser line once", () => {
    expect(textOf(fromHtml("one<div><br></div><div>two</div>"))).toBe("one\n\ntwo");
  });

  it("ignores the filler break that ends a line", () => {
    expect(textOf(fromHtml("hello<br>"))).toBe("hello");
    expect(textOf(fromHtml("hello<br><br>"))).toBe("hello\n");
    expect(textOf(fromHtml("hello<br><div>world</div>"))).toBe("hello\nworld");
  });

  it("round-trips a trailing newline through renderParts", () => {
    const editor = editorWith();
    renderParts(editor, [{ type: "text", text: "hello\n" }], () => {});
    expect(readParts(editor)).toEqual([{ type: "text", text: "hello\n" }]);
  });
});

describe("deleteLineBreak", () => {
  const select = (node: Node, offset: number) => {
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(node, offset));
  };

  it("removes one break when the caret sits at the start of a browser block line", () => {
    const editor = editorWith();
    editor.innerHTML = "one<div>two</div>";
    const line = editor.querySelector("div")?.firstChild as Text;
    select(line, 0);

    expect(deleteLineBreak(editor, "back", noopHandlers)).toEqual([{ type: "text", text: "onetwo" }]);
    expect(readParts(editor)).toEqual([{ type: "text", text: "onetwo" }]);
  });

  it("takes exactly one break out of a doubled line break", () => {
    const editor = editorWith();
    renderParts(editor, [{ type: "text", text: "one\n\ntwo" }], () => {});
    const tail = [...editor.childNodes].find((node) => node.textContent === "two") as Text;
    select(tail, 0);

    expect(deleteLineBreak(editor, "back", noopHandlers)).toEqual([{ type: "text", text: "one\ntwo" }]);
  });

  it("deletes the break ahead of the caret on a forward delete", () => {
    const editor = editorWith();
    renderParts(editor, [{ type: "text", text: "one\ntwo" }], () => {});
    const head = editor.firstChild as Text;
    select(head, 3);

    expect(deleteLineBreak(editor, "forward", noopHandlers)).toEqual([{ type: "text", text: "onetwo" }]);
  });

  it("leaves ordinary character deletes to the browser", () => {
    const editor = editorWith();
    renderParts(editor, [{ type: "text", text: "one\ntwo" }], () => {});
    const head = editor.firstChild as Text;
    select(head, 2);

    expect(deleteLineBreak(editor, "back", noopHandlers)).toBeNull();
  });

  it("keeps chips and their order when a break is removed", () => {
    const editor = editorWith();
    const mention: ComposerPart = { type: "mention", name: "lib", path: "/proj/src/lib" };
    renderPartsWith(editor, [{ type: "text", text: "see " }, mention, { type: "text", text: "\nnow" }], noopHandlers);
    const tail = [...editor.childNodes].find((node) => node.textContent === "now") as Text;
    select(tail, 0);

    expect(deleteLineBreak(editor, "back", noopHandlers)).toEqual([
      { type: "text", text: "see " },
      mention,
      { type: "text", text: "now" },
    ]);
  });
});

describe("normaliseEditorDom", () => {
  const select = (node: Node, offset: number) => {
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(node, offset));
  };
  const mention: ComposerPart = { type: "mention", name: "utils.ts", path: "/proj/src/lib/utils.ts" };

  it("lifts a chip out of a browser block line back to the editor's own children", () => {
    // WebKit's answer to a newline typed before a chip: the chip ends up in a
    // <div> of its own, where it is no longer a sibling of the caret.
    const editor = editorWith(document.createTextNode("one"));
    const block = document.createElement("div");
    block.append(chipNode());
    editor.append(block);
    select(block, 1);

    expect(normaliseEditorDom(editor, noopHandlers)).toEqual([{ type: "text", text: "one\n" }, mention]);
    expect(editor.querySelector("div")).toBeNull();
    const chip = editor.querySelector<HTMLElement>("[data-mention-path]");
    expect(chip?.parentNode).toBe(editor);
  });

  it("keeps the caret where it was, beside the chip and reachable by arrow keys", () => {
    const editor = editorWith(document.createTextNode("one"));
    const block = document.createElement("div");
    block.append(chipNode());
    editor.append(block);
    select(block, 1);

    normaliseEditorDom(editor, noopHandlers);

    const range = window.getSelection()?.getRangeAt(0);
    const chip = editor.querySelector<HTMLElement>("[data-mention-path]");
    expect(range).toBeDefined();
    expect(chipBesideCaret("back", range as Range)).toBe(chip);
  });

  it("restores the padding the browser strips from around a chip", () => {
    const chip = chipNode();
    const editor = editorWith(document.createTextNode("see "), chip);
    select(editor, 2);

    expect(normaliseEditorDom(editor, noopHandlers)).toEqual([{ type: "text", text: "see " }, mention]);
    const rendered = editor.querySelector<HTMLElement>("[data-mention-path]") as HTMLElement;
    expect(rendered.previousSibling?.textContent).toBe("");
    expect(rendered.nextSibling?.textContent).toBe("");
  });

  it("does nothing to an already-flat editor", () => {
    const editor = editorWith();
    renderPartsWith(editor, [{ type: "text", text: "one\n" }, mention], noopHandlers);
    const before = editor.innerHTML;
    select(editor.firstChild as Text, 1);

    expect(normaliseEditorDom(editor, noopHandlers)).toBeNull();
    expect(editor.innerHTML).toBe(before);
  });

  it("leaves the DOM alone when there is no caret to restore", () => {
    const editor = editorWith(document.createTextNode("one"));
    const block = document.createElement("div");
    block.append(chipNode());
    editor.append(block);
    window.getSelection()?.removeAllRanges();

    expect(normaliseEditorDom(editor, noopHandlers)).toBeNull();
    expect(editor.querySelector("div")).not.toBeNull();
  });
});

describe("insertLineBreak", () => {
  const select = (node: Node, offset: number) => {
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(node, offset));
  };

  it("adds a single break before a chip and leaves the caret between the two", () => {
    const editor = editorWith();
    const mention: ComposerPart = { type: "mention", name: "lib", path: "/proj/src/lib" };
    renderPartsWith(editor, [{ type: "text", text: "look at " }, mention], noopHandlers);
    const chip = editor.querySelector("[data-mention-path]") as HTMLElement;
    placeCaretBesideChip(chip, "before");

    expect(insertLineBreak(editor, noopHandlers)).toEqual([{ type: "text", text: "look at \n" }, mention]);
    // One break, not the two WebKit writes beside a contenteditable=false chip.
    expect(editor.querySelectorAll("br")).toHaveLength(1);

    const range = window.getSelection()?.getRangeAt(0);
    expect(chipBesideCaret("forward", range as Range)).toBe(editor.querySelector("[data-mention-path]"));
  });

  it("splits the text at the caret", () => {
    const editor = editorWith();
    renderPartsWith(editor, [{ type: "text", text: "onetwo" }], noopHandlers);
    select(editor.firstChild as Text, 3);

    expect(insertLineBreak(editor, noopHandlers)).toEqual([{ type: "text", text: "one\ntwo" }]);
  });

  it("leaves the keystroke to the browser when there is no caret", () => {
    const editor = editorWith();
    renderPartsWith(editor, [{ type: "text", text: "one" }], noopHandlers);
    window.getSelection()?.removeAllRanges();

    expect(insertLineBreak(editor, noopHandlers)).toBeNull();
  });
});

describe("deleteToLineEdge", () => {
  const select = (node: Node, offset: number) => {
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(node, offset));
  };

  it("takes the whole line back to its start, chip included", () => {
    const editor = editorWith();
    const mention: ComposerPart = { type: "mention", name: "lib", path: "/proj/src/lib" };
    renderPartsWith(
      editor,
      [{ type: "text", text: "one\nsee " }, mention, { type: "text", text: " now" }],
      noopHandlers,
    );
    const tail = [...editor.childNodes].find((node) => node.textContent === " now") as Text;
    select(tail, 4);

    expect(deleteToLineEdge(editor, "back", noopHandlers)).toEqual([{ type: "text", text: "one\n" }]);
    expect(readParts(editor)).toEqual([{ type: "text", text: "one\n" }]);
  });

  it("stops at the line break rather than eating the line above", () => {
    const editor = editorWith();
    renderPartsWith(editor, [{ type: "text", text: "one\ntwo\nthree" }], noopHandlers);
    const line = [...editor.childNodes].find((node) => node.textContent === "two") as Text;
    select(line, 3);

    expect(deleteToLineEdge(editor, "back", noopHandlers)).toEqual([{ type: "text", text: "one\n\nthree" }]);
  });

  it("takes the rest of the line forward", () => {
    const editor = editorWith();
    renderPartsWith(editor, [{ type: "text", text: "one\ntwo" }], noopHandlers);
    const head = editor.firstChild as Text;
    select(head, 1);

    expect(deleteToLineEdge(editor, "forward", noopHandlers)).toEqual([{ type: "text", text: "o\ntwo" }]);
  });

  it("is a no-op at the edge it deletes towards", () => {
    const editor = editorWith();
    renderPartsWith(editor, [{ type: "text", text: "one\ntwo" }], noopHandlers);
    const line = [...editor.childNodes].find((node) => node.textContent === "two") as Text;
    select(line, 0);

    expect(deleteToLineEdge(editor, "back", noopHandlers)).toBeNull();
    expect(readParts(editor)).toEqual([{ type: "text", text: "one\ntwo" }]);
  });
});

describe("renderParts", () => {
  it("round-trips text, newlines, and mentions through readParts", () => {
    const editor = editorWith();
    const parts: ComposerPart[] = [
      { type: "text", text: "look at " },
      { type: "mention", name: "lib", path: "/proj/src/lib" },
      { type: "text", text: " and\nthe next line" },
    ];

    renderParts(editor, parts, () => {});

    const chip = editor.querySelector<HTMLElement>("[data-mention-path]");
    expect(chip?.dataset.mentionPath).toBe("/proj/src/lib");
    expect(readParts(editor)).toEqual(parts);
  });

  it("replaces any previous content", () => {
    const editor = editorWith(document.createTextNode("old text"), chipNode());

    renderParts(editor, [{ type: "text", text: "new" }], () => {});

    expect(readParts(editor)).toEqual([{ type: "text", text: "new" }]);
  });
});

const readyAttachment = (over: Partial<AttachmentPart> = {}): AttachmentPart => ({
  type: "attachment",
  id: "a1",
  filename: "diagram.png",
  mime: "image/png",
  size: 2048,
  path: "/home/user/.codex/staging/a1__diagram.png",
  kind: "image",
  state: "ready",
  ...over,
});

const noopHandlers = { onRemove: () => {}, onRetry: () => {} };

describe("attachment parts", () => {
  it("round-trips a mixed text/mention/attachment sequence in order", () => {
    const editor = editorWith();
    const parts: ComposerPart[] = [
      { type: "text", text: "see " },
      readyAttachment(),
      { type: "text", text: " and " },
      { type: "mention", name: "lib", path: "/proj/src/lib" },
      { type: "text", text: " here" },
    ];

    renderPartsWith(editor, parts, noopHandlers);

    expect(readParts(editor)).toEqual(parts);
  });

  it("preserves a file attachment's fields and position", () => {
    const editor = editorWith();
    const file = readyAttachment({
      id: "f9",
      filename: "notes.md",
      mime: "text/markdown",
      kind: "file",
      path: "/tmp/staging/f9__notes.md",
    });
    renderPartsWith(editor, [file, { type: "text", text: "review" }], noopHandlers);

    const parts = readParts(editor);
    expect(parts[0]).toEqual(file);
    expect(parts[1]).toEqual({ type: "text", text: "review" });
  });

  it("updateAttachmentChip swaps staging for ready without touching neighbours", () => {
    const editor = editorWith();
    const staging = readyAttachment({ state: "staging", path: "", size: 0 });
    renderPartsWith(editor, [{ type: "text", text: "x" }, staging, { type: "text", text: "y" }], noopHandlers);

    updateAttachmentChip(editor, "a1", readyAttachment(), noopHandlers);

    const parts = readParts(editor);
    expect(parts).toEqual([{ type: "text", text: "x" }, readyAttachment(), { type: "text", text: "y" }]);
  });

  it("insertAttachmentChip drops the chip at the caret", () => {
    const text = document.createTextNode("look ");
    const editor = editorWith(text);
    const selection = window.getSelection();
    const range = document.createRange();
    range.setStart(text, 5);
    range.collapse(true);
    selection?.removeAllRanges();
    selection?.addRange(range);

    insertAttachmentChip(readyAttachment(), noopHandlers);

    const parts = readParts(editor);
    expect(parts[0]).toEqual({ type: "text", text: "look " });
    expect(parts[1]).toEqual(readyAttachment());
  });
});

describe("detectQueries", () => {
  /** Put the caret at the end of `text` in a fresh editor and read the queries. */
  function queriesAfter(text: string, cwd = "/proj") {
    const node = document.createTextNode(text);
    const editor = editorWith(node);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(node, text.length));
    return detectQueries(editor, cwd);
  }

  it("detects a $-skill query anywhere after whitespace", () => {
    expect(queriesAfter("$brow")?.skillQuery).toBe("brow");
    expect(queriesAfter("please use $brow")?.skillQuery).toBe("brow");
    expect(queriesAfter("$")?.skillQuery).toBe("");
  });

  it("keeps the colon in a namespaced skill name", () => {
    // `browser-use:browser` is one name, so `:` must not end the query.
    expect(queriesAfter("$browser-use:brow")?.skillQuery).toBe("browser-use:brow");
  });

  it("ignores a $ that is not at a word boundary", () => {
    expect(queriesAfter("cost$5")?.skillQuery).toBeNull();
  });

  it("returns a range covering the trigger and query, so the chip replaces both", () => {
    const node = document.createTextNode("use $brow");
    const editor = editorWith(node);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(caret(node, node.length));
    const detected = detectQueries(editor, "/proj");
    expect(detected?.skillRange?.toString()).toBe("$brow");
  });

  it("does not treat a $ query as a mention, or vice versa", () => {
    const skill = queriesAfter("$brow");
    expect(skill?.mentionQuery).toBeNull();
    const mention = queriesAfter("@utils");
    expect(mention?.skillQuery).toBeNull();
    expect(mention?.mentionQuery).toBe("utils");
  });

  it("still detects a slash command alongside", () => {
    expect(queriesAfter("/pla")?.slashQuery).toBe("pla");
  });
});

describe("buildTurnInput", () => {
  it("maps ready images to localImage and mentions to cwd-relative links, in order", () => {
    const input = buildTurnInput(
      [{ type: "text", text: "here " }, readyAttachment(), { type: "mention", name: "lib", path: "/proj/lib" }],
      "/proj",
    );
    expect(input).toEqual([
      { type: "text", text: "here " },
      { type: "localImage", path: "/home/user/.codex/staging/a1__diagram.png" },
      { type: "text", text: "[lib](lib)" },
    ]);
  });

  it("tolerates a trailing slash on cwd when relativising a mention", () => {
    expect(buildTurnInput([{ type: "mention", name: "utils.ts", path: "/proj/src/utils.ts" }], "/proj/")).toEqual([
      { type: "text", text: "[utils.ts](src/utils.ts)" },
    ]);
  });

  it("keeps a mention absolute when it lies outside cwd", () => {
    expect(buildTurnInput([{ type: "mention", name: "notes.md", path: "/elsewhere/notes.md" }], "/proj")).toEqual([
      { type: "text", text: "[notes.md](/elsewhere/notes.md)" },
    ]);
  });

  it("renders non-image files as a labelled path reference", () => {
    const input = buildTurnInput([readyAttachment({ kind: "file", filename: "notes.md", path: "/tmp/notes.md" })]);
    expect(input).toEqual([{ type: "text", text: "\n[Attached file: notes.md — /tmp/notes.md]\n" }]);
  });

  it("drops attachments that are still staging or failed", () => {
    const input = buildTurnInput([
      { type: "text", text: "hi" },
      readyAttachment({ state: "staging" }),
      readyAttachment({ id: "b", state: "failed" }),
    ]);
    expect(input).toEqual([{ type: "text", text: "hi" }]);
  });

  it("falls back to a single empty text part when nothing is sendable", () => {
    expect(buildTurnInput([readyAttachment({ state: "failed" })])).toEqual([{ type: "text", text: "" }]);
  });

  it("sends a skill as the native protocol item, not as text", () => {
    // Unlike a file mention, `skill` is a real turn-input variant, so it needs
    // no markdown-link workaround — and the label never goes to the model.
    const input = buildTurnInput(
      [
        { type: "text", text: "use " },
        { type: "skill", name: "browser-use:browser", label: "Browser" },
      ],
      "/proj",
    );
    expect(input).toEqual([
      { type: "text", text: "use " },
      { type: "skill", name: "browser-use:browser" },
    ]);
  });
});

describe("hasSendableContent", () => {
  it("treats a lone skill chip as sendable", () => {
    // "$browser" with no prose is a legitimate turn.
    expect(hasSendableContent([{ type: "skill", name: "browser", label: "Browser" }])).toBe(true);
  });

  it("is true for text, mentions, or a ready attachment; false otherwise", () => {
    expect(hasSendableContent([{ type: "text", text: "  " }])).toBe(false);
    expect(hasSendableContent([{ type: "text", text: "hi" }])).toBe(true);
    expect(hasSendableContent([readyAttachment({ state: "staging" })])).toBe(false);
    expect(hasSendableContent([readyAttachment()])).toBe(true);
    expect(hasSendableContent([{ type: "mention", name: "a", path: "/a" }])).toBe(true);
  });
});

describe("formatSize", () => {
  it("formats bytes, kilobytes, and megabytes", () => {
    expect(formatSize(512)).toBe("512 B");
    expect(formatSize(2048)).toBe("2 KB");
    expect(formatSize(3_500_000)).toBe("3.3 MB");
  });
});
