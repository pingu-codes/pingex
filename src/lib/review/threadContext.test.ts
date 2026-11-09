import { describe, expect, it } from "vitest";
import type { CommentThread } from "$lib/review/review";
import { CONTEXT_ROWS, contextRows, MAX_ROWS, threadLocation } from "$lib/review/threadContext";
import type { DiffHunk, DiffLine } from "$lib/types";

/** A hunk from unified-diff rows (`+`, `-` or ` ` then the code), numbered like `parse_patch`. */
function hunk(oldStart: number, newStart: number, rows: string[]): DiffHunk {
  let oldLine = oldStart;
  let newLine = newStart;
  const lines: DiffLine[] = rows.map((row) => {
    const content = row.slice(1);
    if (row[0] === "+") return { kind: "add", content, oldLine: null, newLine: newLine++ };
    if (row[0] === "-") return { kind: "del", content, oldLine: oldLine++, newLine: null };
    return { kind: "context", content, oldLine: oldLine++, newLine: newLine++ };
  });
  return { header: `@@ -${oldStart} +${newStart} @@`, oldStart, oldLines: 0, newStart, newLines: 0, lines };
}

function thread(overrides: Partial<CommentThread> = {}): CommentThread {
  return {
    key: "T1",
    path: "notes.txt",
    line: 13,
    side: "RIGHT",
    resolved: false,
    comments: [],
    startLine: null,
    originalLine: 13,
    originalStartLine: null,
    diffHunk: null,
    outdated: false,
    ...overrides,
  };
}

// Head lines 10..15 after one removed base line (old 11).
const HUNK = hunk(10, 10, [" ten", "-old eleven", "+eleven", "+twelve", " thirteen", "+fourteen", "+fifteen"]);

const text = (rows: ReturnType<typeof contextRows>) => rows?.map((row) => row.line.content);

describe("contextRows", () => {
  it("is null without a hunk, so the thread shows only its place", () => {
    expect(contextRows(thread())).toBeNull();
  });

  it("ends a single-line comment's rows at its original line, with lines before it", () => {
    const rows = contextRows(thread({ diffHunk: HUNK, originalLine: 13, line: 40 }));
    expect(rows).toHaveLength(CONTEXT_ROWS);
    expect(text(rows)).toEqual(["old eleven", "eleven", "twelve", "thirteen"]);
    expect(rows?.map((row) => row.commented)).toEqual([false, false, false, true]);
  });

  it("shows a multi-line comment's whole original range", () => {
    const rows = contextRows(thread({ diffHunk: HUNK, originalStartLine: 11, originalLine: 15 }));
    expect(text(rows)).toEqual(["eleven", "twelve", "thirteen", "fourteen", "fifteen"]);
    expect(rows?.every((row) => row.commented)).toBe(true);
  });

  it("numbers a LEFT comment on the base side", () => {
    const rows = contextRows(thread({ diffHunk: HUNK, side: "LEFT", originalLine: 11 }));
    expect(text(rows)).toEqual(["ten", "old eleven"]);
    expect(rows?.at(-1)?.commented).toBe(true);
  });

  it("falls back to the hunk's end when the line is not in it", () => {
    const rows = contextRows(thread({ diffHunk: HUNK, originalLine: 99 }));
    expect(text(rows)?.at(-1)).toBe("fifteen");
  });

  it("caps a long range, keeping its end", () => {
    const long = hunk(
      1,
      1,
      Array.from({ length: 30 }, (_, index) => ` line ${index + 1}`),
    );
    const rows = contextRows(thread({ diffHunk: long, originalStartLine: 2, originalLine: 30 }));
    expect(rows).toHaveLength(MAX_ROWS);
    expect(text(rows)?.at(-1)).toBe("line 30");
    expect(rows?.every((row) => row.commented)).toBe(true);
  });

  it("highlights code in a language it knows", () => {
    const code = hunk(1, 1, ["+const answer = 42;"]);
    const [row] = contextRows(thread({ path: "a.ts", diffHunk: code, originalLine: 1 })) ?? [];
    expect(row.html).toContain("hljs-keyword");
    const [plain] = contextRows(thread({ diffHunk: code, originalLine: 1 })) ?? [];
    expect(plain.html).toBeNull();
  });
});

describe("threadLocation", () => {
  it("names the head line, or the range", () => {
    expect(threadLocation(thread({ path: "a.rs", line: 7 }))).toBe("a.rs:7");
    expect(threadLocation(thread({ path: "a.rs", startLine: 3, line: 7 }))).toBe("a.rs:3–7");
    expect(threadLocation(thread({ path: "a.rs", startLine: 7, line: 7 }))).toBe("a.rs:7");
    expect(threadLocation(thread({ path: "a.rs", line: null }))).toBe("a.rs:file");
  });

  it("names the original lines of an outdated thread", () => {
    const outdated = thread({ path: "a.rs", outdated: true, line: 141, originalLine: 141, originalStartLine: 137 });
    expect(threadLocation(outdated)).toBe("a.rs:137–141");
  });
});
