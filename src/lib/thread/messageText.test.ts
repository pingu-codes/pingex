import { describe, expect, it } from "vitest";
import { mergeTextParts, messageText, userMessageMarkdown } from "$lib/thread/messageText";
import type { UserInputPart } from "$lib/types";

// A mention leaves the composer as its own text part, so this is what a sent
// "check @utils.ts please" actually looks like on the way back.
const mentioned: UserInputPart[] = [
  { type: "text", text: "check " },
  { type: "text", text: "[utils.ts](src/lib/utils.ts)" },
  { type: "text", text: " please" },
];

describe("mergeTextParts", () => {
  it("joins the runs a mention was split across", () => {
    expect(mergeTextParts(mentioned)).toEqual([{ type: "text", text: "check [utils.ts](src/lib/utils.ts) please" }]);
  });

  it("keeps non-text parts in place", () => {
    const parts: UserInputPart[] = [
      { type: "text", text: "a" },
      { type: "localImage", path: "/tmp/shot.png" },
      { type: "text", text: "b" },
      { type: "text", text: "c" },
    ];
    expect(mergeTextParts(parts)).toEqual([
      { type: "text", text: "a" },
      { type: "localImage", path: "/tmp/shot.png" },
      { type: "text", text: "bc" },
    ]);
  });
});

describe("messageText", () => {
  it("reassembles the prose without inventing newlines", () => {
    expect(messageText(mentioned)).toBe("check [utils.ts](src/lib/utils.ts) please");
  });
});

describe("userMessageMarkdown", () => {
  it("keeps mentions as links, rewritten to ./-relative paths", () => {
    expect(userMessageMarkdown(mentioned, "/proj")).toBe("check [utils.ts](./src/lib/utils.ts) please");
  });

  it("leaves prose links and absolute paths alone", () => {
    const parts: UserInputPart[] = [{ type: "text", text: "see [the docs](https://example.com)" }];
    expect(userMessageMarkdown(parts, "/proj")).toBe("see [the docs](https://example.com)");
    expect(userMessageMarkdown([{ type: "text", text: "[a.ts](/elsewhere/a.ts)" }], "/proj")).toBe(
      "[a.ts](/elsewhere/a.ts)",
    );
  });

  it("renders attachments and images as links of the same shape", () => {
    const parts: UserInputPart[] = [
      { type: "text", text: "look" },
      { type: "localImage", path: "/proj/shot.png" },
      { type: "image", url: "https://example.com/a.png" },
      { type: "skill", name: "review" },
    ];
    expect(userMessageMarkdown(parts, "/proj")).toBe(
      "look\n[shot.png](./shot.png)\n[image](https://example.com/a.png)\n@review",
    );
  });

  it("is empty for a message with nothing in it", () => {
    expect(userMessageMarkdown([], "/proj")).toBe("");
  });
});
