// @vitest-environment node
import { describe, expect, it } from "vitest";
import { chunkRows, type DiffRow, reuseUnchanged, sameFields, sameItems } from "$lib/review/reviewPage";

function rows(count: number, content = (index: number) => `line ${index}`): DiffRow[] {
  return Array.from({ length: count }, (_, index) => ({
    kind: "line",
    key: `h0l${index}`,
    line: { kind: "context", content: content(index), oldLine: index + 1, newLine: index + 1 },
    side: "RIGHT",
    number: index + 1,
  }));
}

describe("chunkRows", () => {
  it("cuts rows into runs keyed by their first row", () => {
    const chunks = chunkRows(rows(130), 60);
    expect(chunks.map((chunk) => chunk.rows.length)).toEqual([60, 60, 10]);
    expect(chunks.map((chunk) => chunk.key)).toEqual(["h0l0", "h0l60", "h0l120"]);
  });

  it("measures each run's longest line, a tab as eight columns", () => {
    const [chunk] = chunkRows(
      rows(3, (index) => (index === 1 ? "\tab" : "a")),
      60,
    );
    expect(chunk.columns).toBe(10);
  });

  it("counts a hunk header as a row of its own width", () => {
    const [chunk] = chunkRows([{ kind: "hunk", key: "h0", header: "@@ -1,2 +1,3 @@" }]);
    expect(chunk).toMatchObject({ key: "h0", columns: 15 });
  });

  it("has no runs for no rows", () => {
    expect(chunkRows([])).toEqual([]);
  });
});

describe("reuseUnchanged", () => {
  it("keeps the previous value where it is equal, and takes the new one where not", () => {
    const a = { id: 1 };
    const b = { id: 2 };
    const previous = new Map([
      ["kept", [a]],
      ["changed", [a]],
    ]);
    const next = reuseUnchanged(
      previous,
      new Map([
        ["kept", [a]],
        ["changed", [a, b]],
        ["new", [b]],
      ]),
      sameItems,
    );
    expect(next.get("kept")).toBe(previous.get("kept"));
    expect(next.get("changed")).toEqual([a, b]);
    expect(next.get("new")).toEqual([b]);
    expect(next.has("gone")).toBe(false);
  });

  it("compares objects field by field with sameFields", () => {
    const previous = new Map([["file", { comments: 1, unresolved: 0 }]]);
    const next = reuseUnchanged(previous, new Map([["file", { comments: 1, unresolved: 0 }]]), sameFields);
    expect(next.get("file")).toBe(previous.get("file"));
    expect(sameFields({ comments: 1 }, { comments: 2 })).toBe(false);
  });
});
