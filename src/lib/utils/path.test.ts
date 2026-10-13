import { describe, expect, it } from "vitest";
import { basename, trimSlash } from "$lib/utils/path";

describe("trimSlash", () => {
  it("drops trailing separators of either kind", () => {
    expect(trimSlash("/a/b/")).toBe("/a/b");
    expect(trimSlash("/a/b//")).toBe("/a/b");
    expect(trimSlash("C:\\a\\b\\")).toBe("C:\\a\\b");
    expect(trimSlash("/a/b")).toBe("/a/b");
  });

  it("empties a bare root, so joins don't double the separator", () => {
    expect(trimSlash("/")).toBe("");
  });
});

describe("basename", () => {
  it("reads the final segment", () => {
    expect(basename("/a/b/c")).toBe("c");
    expect(basename("/a/b/c/")).toBe("c");
    expect(basename("src/main.rs")).toBe("main.rs");
    expect(basename("main.rs")).toBe("main.rs");
  });

  it("splits Windows paths", () => {
    expect(basename("C:\\Users\\me\\repo")).toBe("repo");
    expect(basename("C:\\Users\\me\\repo\\")).toBe("repo");
    expect(basename("\\\\wsl$\\Ubuntu\\home/me/x.ts")).toBe("x.ts");
  });

  it("returns a path with no segment unchanged", () => {
    expect(basename("/")).toBe("/");
    expect(basename("")).toBe("");
  });
});
