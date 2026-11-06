import { describe, expect, it } from "vitest";
import { diffRows } from "$lib/review/reviewPage";
import type { DiffLine, PrFile } from "$lib/types";
import { splitHighlighted } from "$lib/utils/markdown";
import { highlightedRows } from "./diffHighlight";

function file(path: string, lines: Array<[DiffLine["kind"], string]>, overrides: Partial<PrFile> = {}): PrFile {
  let oldLine = 1;
  let newLine = 1;
  const diffLines: DiffLine[] = lines.map(([kind, content]) => ({
    kind,
    content,
    oldLine: kind === "add" ? null : oldLine++,
    newLine: kind === "del" ? null : newLine++,
  }));
  return {
    path,
    oldPath: null,
    status: "modified",
    additions: 0,
    deletions: 0,
    patch: "",
    hunks: [
      {
        header: "@@ -1 +1 @@",
        oldStart: 1,
        oldLines: oldLine - 1,
        newStart: 1,
        newLines: newLine - 1,
        lines: diffLines,
      },
    ],
    patchTruncated: false,
    blobSha: null,
    ...overrides,
  };
}

/** Parse a fragment on its own, as `{@html}` does. */
function parse(html: string): HTMLElement {
  const host = document.createElement("span");
  host.innerHTML = html;
  return host;
}

describe("splitHighlighted", () => {
  it("closes spans at each newline and reopens them on the next line", () => {
    const html = 'a<span class="c">/* one\ntwo\n*/</span> b\n\nend';
    expect(splitHighlighted(html)).toEqual([
      'a<span class="c">/* one</span>',
      '<span class="c">two</span>',
      '<span class="c">*/</span> b',
      "",
      "end",
    ]);
  });

  it("keeps nested spans nested", () => {
    const html = '<span class="s">`x <span class="t">y\nz</span>`</span>';
    expect(splitHighlighted(html)).toEqual([
      '<span class="s">`x <span class="t">y</span></span>',
      '<span class="s"><span class="t">z</span>`</span>',
    ]);
  });
});

describe("highlightedRows", () => {
  it("colours each row in the file's language, keeping its text", () => {
    const changed = file("src/a.ts", [
      ["context", "const a = 1;"],
      ["del", 'let b = "old";'],
      ["add", 'let b = "new";'],
    ]);
    const rows = highlightedRows(changed)!;
    for (const row of diffRows(changed)) {
      if (row.kind !== "line") continue;
      const html = rows.get(row.key)!;
      expect(parse(html).textContent).toBe(row.line.content);
    }
    expect(parse(rows.get("h0l0")!).querySelector(".hljs-keyword")?.textContent).toBe("const");
    expect(parse(rows.get("h0l1")!).querySelector(".hljs-string")?.textContent).toBe('"old"');
    expect(parse(rows.get("h0l2")!).querySelector(".hljs-string")?.textContent).toBe('"new"');
  });

  it("carries a block comment across rows, each side on its own", () => {
    const changed = file("src/a.ts", [
      ["add", "/* opened"],
      ["del", "const removed = 1;"],
      ["context", "still a comment */"],
      ["context", "const after = 2;"],
    ]);
    const rows = highlightedRows(changed)!;
    expect(parse(rows.get("h0l0")!).querySelector(".hljs-comment")).not.toBeNull();
    // The removed line reads as base code: the comment opened on the head side.
    expect(parse(rows.get("h0l1")!).querySelector(".hljs-comment")).toBeNull();
    expect(parse(rows.get("h0l2")!).querySelector(".hljs-comment")?.textContent).toBe("still a comment */");
    expect(parse(rows.get("h0l3")!).querySelector(".hljs-comment")).toBeNull();
  });

  it("leaves plain text alone", () => {
    expect(highlightedRows(file("notes.txt", [["add", "hello"]]))).toBeNull();
    expect(highlightedRows(file("App.svelte", [["add", "<div></div>"]]))).toBeNull();
    expect(highlightedRows(file("src/a.ts", [["add", "x"]], { patchTruncated: true }))).toBeNull();
    expect(highlightedRows(file("src/a.ts", [["add", "x".repeat(200_000)]]))).toBeNull();
  });

  it("memoises per file", () => {
    const changed = file("src/a.ts", [["add", "const a = 1;"]]);
    expect(highlightedRows(changed)).toBe(highlightedRows(changed));
  });
});
