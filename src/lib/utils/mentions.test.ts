import { describe, expect, it } from "vitest";
import {
  copyMentionPath,
  hasMentions,
  relativeMentionPath,
  resolveMentionPath,
  splitMentions,
} from "$lib/utils/mentions";

describe("splitMentions", () => {
  it("recovers a mention Codex persisted as a markdown link", () => {
    expect(splitMentions("add comments to [index.ts](packages/cli/index.ts) please")).toEqual([
      { type: "text", text: "add comments to " },
      { type: "mention", name: "index.ts", path: "packages/cli/index.ts" },
      { type: "text", text: " please" },
    ]);
  });

  it("recovers directory mentions written with a trailing slash", () => {
    expect(splitMentions("[cli](packages/cli/)")).toEqual([{ type: "mention", name: "cli", path: "packages/cli/" }]);
  });

  it("leaves prose links alone", () => {
    const text = "see [the docs](https://example.com/docs) and [the plan](#plan)";
    expect(splitMentions(text)).toEqual([{ type: "text", text }]);
    expect(hasMentions(text)).toBe(false);
  });

  it("leaves links whose label is not the file name alone", () => {
    const text = "read [this file](src/lib/utils.ts)";
    expect(splitMentions(text)).toEqual([{ type: "text", text }]);
  });

  it("handles several mentions in one message", () => {
    expect(splitMentions("[a.ts](src/a.ts) vs [b.ts](src/b.ts)")).toEqual([
      { type: "mention", name: "a.ts", path: "src/a.ts" },
      { type: "text", text: " vs " },
      { type: "mention", name: "b.ts", path: "src/b.ts" },
    ]);
  });
});

describe("resolveMentionPath", () => {
  it("joins relative paths onto the thread cwd", () => {
    expect(resolveMentionPath("src/a.ts", "/repo/")).toBe("/repo/src/a.ts");
  });

  it("keeps absolute paths and survives a missing cwd", () => {
    expect(resolveMentionPath("/abs/a.ts", "/repo")).toBe("/abs/a.ts");
    expect(resolveMentionPath("src/a.ts", "")).toBe("src/a.ts");
  });
});

describe("relativeMentionPath", () => {
  it("strips the thread cwd, with or without a trailing slash", () => {
    expect(relativeMentionPath("/repo/src/a.ts", "/repo")).toBe("src/a.ts");
    expect(relativeMentionPath("/repo/src/a.ts", "/repo/")).toBe("src/a.ts");
  });

  it("leaves paths outside cwd alone, and survives a missing cwd", () => {
    expect(relativeMentionPath("/elsewhere/a.ts", "/repo")).toBe("/elsewhere/a.ts");
    expect(relativeMentionPath("/repo-other/a.ts", "/repo")).toBe("/repo-other/a.ts");
    expect(relativeMentionPath("/repo/src/a.ts", "")).toBe("/repo/src/a.ts");
  });

  it("prefixes ./ for the clipboard, leaving absolute and already-relative paths", () => {
    expect(copyMentionPath("/repo/src/a.ts", "/repo")).toBe("./src/a.ts");
    expect(copyMentionPath("src/a.ts", "/repo")).toBe("./src/a.ts");
    expect(copyMentionPath("./src/a.ts", "/repo")).toBe("./src/a.ts");
    expect(copyMentionPath("../sibling/a.ts", "/repo")).toBe("../sibling/a.ts");
    expect(copyMentionPath("/elsewhere/a.ts", "/repo")).toBe("/elsewhere/a.ts");
  });

  it("round-trips with resolveMentionPath", () => {
    const path = "/repo/src/lib/a.ts";
    expect(resolveMentionPath(relativeMentionPath(path, "/repo"), "/repo")).toBe(path);
  });
});
