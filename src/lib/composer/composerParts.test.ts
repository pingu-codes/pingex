import { describe, expect, it } from "vitest";
import {
  type AttachmentPart,
  buildTurnInput,
  formatSize,
  hasSendableContent,
  normaliseParts,
} from "$lib/composer/composerParts";

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

describe("normaliseParts", () => {
  it("drops empty text, merges adjacent text and never returns nothing", () => {
    expect(
      normaliseParts([
        { type: "text", text: "" },
        { type: "text", text: "a" },
        { type: "text", text: "b" },
        { type: "mention", name: "x", path: "/x" },
        { type: "text", text: "" },
      ]),
    ).toEqual([
      { type: "text", text: "ab" },
      { type: "mention", name: "x", path: "/x" },
    ]);
    expect(normaliseParts([])).toEqual([{ type: "text", text: "" }]);
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
        { type: "skill", name: "browser-use:browser", path: "/skills/browser/SKILL.md", label: "Browser" },
      ],
      "/proj",
    );
    expect(input).toEqual([
      { type: "text", text: "use " },
      { type: "skill", name: "browser-use:browser", path: "/skills/browser/SKILL.md" },
    ]);
  });
});

describe("hasSendableContent", () => {
  it("treats a lone skill chip as sendable", () => {
    // "$browser" with no prose is a legitimate turn.
    expect(hasSendableContent([{ type: "skill", name: "browser", path: "/s", label: "Browser" }])).toBe(true);
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
