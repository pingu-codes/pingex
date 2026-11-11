import { describe, expect, it } from "vitest";
import { type ChangeNote, noteEntry, notesMessage } from "./notes";

function note(extra: Partial<ChangeNote> = {}): ChangeNote {
  return {
    id: "n1",
    root: "/code/api",
    repoLabel: "api",
    segmentId: "pr:12",
    segmentLabel: "branch `feat/auth` (#12)",
    path: "src/auth.ts",
    side: "RIGHT",
    startLine: null,
    line: 10,
    excerpt: "const token = read();",
    body: "Handle a missing token here.",
    replacement: null,
    ...extra,
  };
}

describe("notesMessage", () => {
  it("anchors one note to its repository, segment and line", () => {
    expect(notesMessage([note()])).toBe(
      [
        "I left a note on your changes:",
        "**api** (`/code/api`), branch `feat/auth` (#12)",
        "- `src/auth.ts:10`\n  > const token = read();\n  Handle a missing token here.",
      ].join("\n\n"),
    );
  });

  it("groups notes by repository and segment, keeping their order", () => {
    const message = notesMessage([
      note({ id: "1", line: 3 }),
      note({
        id: "2",
        root: "/code/web",
        repoLabel: "web",
        segmentId: "uncommitted",
        segmentLabel: "uncommitted changes",
      }),
      note({ id: "3", line: 20, startLine: 18 }),
    ]);
    expect(message.startsWith("I left 3 notes on your changes:")).toBe(true);
    const api = message.indexOf("**api**");
    const web = message.indexOf("**web**");
    expect(api).toBeLessThan(web);
    expect(message.indexOf("src/auth.ts:18-20")).toBeLessThan(web);
    expect(message.match(/\*\*api\*\*/g)).toHaveLength(1);
  });

  it("says when a note is on removed lines, and fences a suggestion safely", () => {
    const message = notesMessage([note({ side: "LEFT", replacement: "const a = `x`;\n```\n" })]);
    expect(message).toContain("`src/auth.ts:10` (removed lines)");
    expect(message).toContain("  Suggested replacement:\n  ````\n  const a = `x`;");
  });

  it("cuts a long excerpt", () => {
    const excerpt = Array.from({ length: 10 }, (_, index) => `line ${index}`).join("\n");
    const message = notesMessage([note({ excerpt })]);
    expect(message).toContain("  > line 5\n  > … 4 more lines");
    expect(message).not.toContain("line 6");
  });

  it("is empty with no notes", () => {
    expect(notesMessage([])).toBe("");
  });
});

describe("noteEntry", () => {
  it("draws as a comment or a suggestion under its line", () => {
    expect(noteEntry(note())).toEqual({
      kind: "comment",
      id: "n1",
      anchor: { path: "src/auth.ts", side: "RIGHT", line: 10, startLine: null },
      body: "Handle a missing token here.",
    });
    expect(noteEntry(note({ replacement: "x" })).kind).toBe("suggestion");
  });
});
