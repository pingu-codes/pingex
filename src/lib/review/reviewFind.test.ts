import { describe, expect, it } from "vitest";
import { computeRows } from "$lib/review/diffHighlight";
import type { CommentThread } from "$lib/review/review";
import {
  domUnits,
  type FindUnit,
  fileFindUnits,
  findingTexts,
  findMatches,
  MAX_MATCHES,
  markdownText,
} from "$lib/review/reviewFind";
import { diffRows } from "$lib/review/reviewPage";
import type { Finding, PendingEntry, PrFile } from "$lib/types";

function file(path: string, lines: { kind?: "add" | "del" | "context"; content: string }[]): PrFile {
  return {
    path,
    oldPath: null,
    status: "modified",
    additions: 0,
    deletions: 0,
    patch: "",
    blobSha: null,
    patchTruncated: false,
    hunks: [
      {
        header: "@@ -1 +1 @@ function load()",
        oldStart: 1,
        oldLines: 1,
        newStart: 1,
        newLines: 1,
        lines: lines.map((line, index) => {
          const kind = line.kind ?? "context";
          return {
            kind,
            content: line.content,
            oldLine: kind === "add" ? null : index + 1,
            newLine: kind === "del" ? null : index + 1,
          };
        }),
      },
    ],
  } as PrFile;
}

function thread(line: number | null, body: string, id = 1): CommentThread {
  return {
    key: `t${id}`,
    path: "a.ts",
    line,
    side: "RIGHT",
    resolved: false,
    comments: [{ id, body }],
  } as unknown as CommentThread;
}

const units = (list: { id: string; text: string }[]): FindUnit[] =>
  list.map(({ id, text }) => ({ id, text, path: "a.ts", row: null }));

describe("findMatches", () => {
  it("finds every occurrence, in unit order, with offsets into the unit's text", () => {
    const { matches, capped } = findMatches(
      units([
        { id: "one", text: "foo bar foo" },
        { id: "two", text: "nothing" },
        { id: "three", text: "a foo" },
      ]),
      "foo",
      false,
    );
    expect(matches.map((match) => [match.unit.id, match.start, match.end])).toEqual([
      ["one", 0, 3],
      ["one", 8, 11],
      ["three", 2, 5],
    ]);
    expect(matches.map((match) => match.order)).toEqual([0, 0, 2]);
    expect(capped).toBe(false);
  });

  it("ignores case unless asked not to", () => {
    const text = units([{ id: "one", text: "Foo foo FOO" }]);
    expect(findMatches(text, "foo", false).matches).toHaveLength(3);
    expect(findMatches(text, "foo", true).matches).toHaveLength(1);
  });

  it("treats the query as literal text, not a pattern", () => {
    const text = units([{ id: "one", text: "a.b axb (c) [d] e|f g\\h ^$ {1} a-b" }]);
    for (const [query, count] of [
      ["a.b", 1],
      ["(c)", 1],
      ["[d]", 1],
      ["e|f", 1],
      ["g\\h", 1],
      ["^$", 1],
      ["{1}", 1],
      ["a-b", 1],
    ] as const) {
      expect(findMatches(text, query, false).matches, query).toHaveLength(count);
    }
  });

  it("keeps offsets right when lower-casing would change a string's length", () => {
    // "İ".toLowerCase() is two code units, which would shift a naive search.
    const text = "İİİ needle";
    const { matches } = findMatches(units([{ id: "one", text }]), "needle", false);
    expect(text.slice(matches[0].start, matches[0].end)).toBe("needle");
  });

  it("lets a space in the query match any run of whitespace, as rendered text wraps", () => {
    const { matches } = findMatches(units([{ id: "one", text: "soft\nwrapped  and\ttabbed" }]), "soft wrapped", false);
    expect(matches).toHaveLength(1);
    expect(findMatches(units([{ id: "one", text: "and\ttabbed" }]), "and tabbed", false).matches).toHaveLength(1);
  });

  it("does not trim: a leading space is part of the query", () => {
    const text = units([{ id: "one", text: "afoo foo" }]);
    expect(findMatches(text, " foo", false).matches).toHaveLength(1);
  });

  it("finds nothing for an empty query", () => {
    expect(findMatches(units([{ id: "one", text: "foo" }]), "", false).matches).toEqual([]);
  });

  it("stops at the cap and says so", () => {
    const text = units([{ id: "one", text: "a".repeat(MAX_MATCHES + 5) }]);
    const { matches, capped } = findMatches(text, "a", false);
    expect(matches).toHaveLength(MAX_MATCHES);
    expect(capped).toBe(true);
  });
});

describe("fileFindUnits", () => {
  it("lists a file's text in the order the page shows it", () => {
    const pending: PendingEntry[] = [
      {
        kind: "suggestion",
        id: "p1",
        anchor: { path: "a.ts", side: "RIGHT", line: 1, startLine: null },
        body: "pending note",
        replacement: "better()",
      } as PendingEntry,
    ];
    const list = fileFindUnits(
      file("a.ts", [{ content: "first()" }, { content: "second()" }]),
      [thread(1, "on the first line"), thread(99, "outside the diff", 2)],
      pending,
      (side, line) => (side === "RIGHT" && line === 1 ? [{ id: "finding:f:title", text: "a finding" }] : []),
    );
    expect(list.map((unit) => [unit.id, unit.text, unit.row])).toEqual([
      ["path", "a.ts", null],
      ["h0", "@@ -1 +1 @@ function load()", "h0"],
      ["h0l0", "first()", "h0l0"],
      ["comment:1", "on the first line\n", "h0l0"],
      ["pending:p1", "pending note\n", "h0l0"],
      ["pending:p1:replacement", "better()", "h0l0"],
      ["finding:f:title", "a finding", "h0l0"],
      ["h0l1", "second()", "h0l1"],
      ["comment:2", "outside the diff\n", null],
    ]);
  });

  it("keeps removed and added lines apart when their numbers collide", () => {
    const list = fileFindUnits(
      file("a.ts", [
        { kind: "del", content: "old" },
        { kind: "add", content: "new" },
      ]),
      [{ ...thread(2, "on the added line"), side: "RIGHT" }],
      [],
    );
    expect(list.map((unit) => unit.id)).toEqual(["path", "h0", "h0l0", "h0l1", "comment:1"]);
  });

  it("searches a file without a patch by its path alone", () => {
    const list = fileFindUnits({ ...file("logo.png", []), hunks: [], patchTruncated: true }, [], []);
    expect(list.map((unit) => unit.id)).toEqual(["path"]);
  });
});

describe("markdownText", () => {
  it("is the text a reader sees, not the markdown source", () => {
    expect(markdownText("**bold** [a link](https://example.com)")).toBe("bold a link\n");
  });

  it("matches a rendered body's textContent, so offsets line up", () => {
    const element = document.createElement("div");
    element.innerHTML = "<p>a <code>b</code></p>\n";
    expect(markdownText("a `b`")).toBe(element.textContent);
  });
});

describe("findingTexts", () => {
  const finding = {
    id: "f1",
    title: "A title",
    body: "The body",
    evidence: "Some evidence",
    state: "draft",
  } as Finding;

  it("gives a finding's title, body and evidence", () => {
    expect(findingTexts(finding, { agentLabel: "Codex", addedBody: null, added: false, sameAs: null })).toEqual([
      { id: "finding:f1:title", text: "A title" },
      { id: "finding:f1:body", text: "The body" },
      { id: "finding:f1:evidence", text: "Some evidence" },
    ]);
  });

  it("gives the edited text once added", () => {
    const texts = findingTexts(finding, { agentLabel: "Codex", addedBody: "Edited", added: true, sameAs: null });
    expect(texts[1]).toEqual({ id: "finding:f1:body", text: "Edited" });
  });

  it("gives a merged finding its one line", () => {
    const merged = { ...finding, state: "merged" } as Finding;
    expect(
      findingTexts(merged, { agentLabel: "Codex", addedBody: null, added: false, sameAs: "sam's a.ts:6" }),
    ).toEqual([{ id: "finding:f1:merged", text: "Codex found this too: A title. Same as sam's a.ts:6 comment." }]);
  });
});

describe("domUnits", () => {
  function root(html: string) {
    const element = document.createElement("div");
    element.innerHTML = html;
    return element;
  }

  it("joins inline runs and splits at block elements", () => {
    const list = domUnits(root("<p>foo <b>bar</b> baz</p><p>next</p>"));
    expect(list.map((unit) => unit.text)).toEqual(["foo bar baz", "next"]);
  });

  it("does not match across two blocks", () => {
    expect(findMatches(domUnits(root("<p>foo</p><p>bar</p>")), "foobar", false).matches).toEqual([]);
  });

  it("skips controls and hidden text", () => {
    const list = domUnits(
      root(
        '<button>Reply</button><textarea>draft</textarea><span class="sr-only">Hidden</span><span aria-hidden="true">x</span><p>shown</p>',
      ),
    );
    expect(list.map((unit) => unit.text)).toEqual(["shown"]);
  });

  it("keeps the text nodes, so a match can be painted across them", () => {
    const [unit] = domUnits(root("<p>foo <b>bar</b></p>"));
    expect(unit.nodes?.map((node) => node.data)).toEqual(["foo ", "bar"]);
  });

  it("keeps a diff row's columns apart", () => {
    const list = domUnits(root('<div class="ln"><span>12</span><span>+</span><span>code</span></div>'));
    expect(list.map((unit) => unit.text)).toEqual(["12", "+", "code"]);
  });
});

describe("highlighted rows", () => {
  it("keep the row's exact text, which painting depends on", () => {
    const lines = [
      'const a = "<b>&amp;</b>";',
      "  if (a < 1 && b > 2) {}",
      "\ttabbed\t// comment",
      "",
      "/* open",
      "close */",
    ];
    const source = file("a.ts", [...lines.map((content) => ({ content })), { kind: "del", content: "gone <i>" }]);
    const html = computeRows(source);
    expect(html).not.toBeNull();
    for (const row of diffRows(source)) {
      if (row.kind !== "line") continue;
      const element = document.createElement("span");
      element.innerHTML = html?.get(row.key) ?? "";
      expect(element.textContent).toBe(row.line.content);
    }
  });
});
