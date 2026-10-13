// @vitest-environment node
import { describe, expect, it } from "vitest";
import { parseFileRef } from "./fileRef";

const ref = (path: string, line: number | null = null, column: number | null = null) => ({ path, line, column });

describe("parseFileRef", () => {
  it("leaves a bare path alone", () => {
    expect(parseFileRef("/Users/x/y.csv")).toEqual(ref("/Users/x/y.csv"));
  });

  it("splits a cited line", () => {
    expect(parseFileRef("/a/b.ts:100")).toEqual(ref("/a/b.ts", 100));
  });

  it("splits a cited line and column", () => {
    expect(parseFileRef("/a/b.ts:100:5")).toEqual(ref("/a/b.ts", 100, 5));
  });

  it("stops at two suffixes", () => {
    expect(parseFileRef("/a/b.ts:1:2:3")).toEqual(ref("/a/b.ts:1", 2, 3));
  });

  it("percent-decodes the path", () => {
    expect(parseFileRef("/Users/x/my%20file.csv")).toEqual(ref("/Users/x/my file.csv"));
    expect(parseFileRef("/a/b%20c.ts:9")).toEqual(ref("/a/b c.ts", 9));
  });

  it("survives a malformed escape", () => {
    expect(parseFileRef("/a/100%.ts")).toEqual(ref("/a/100%.ts"));
  });

  it("keeps a home-relative prefix", () => {
    expect(parseFileRef("~/.codex/config.toml")).toEqual(ref("~/.codex/config.toml"));
  });

  it("does not mistake a Windows drive for a line number", () => {
    expect(parseFileRef("C:\\Users\\x\\a.rs")).toEqual(ref("C:\\Users\\x\\a.rs"));
    expect(parseFileRef("C:\\Users\\x\\a.rs:100")).toEqual(ref("C:\\Users\\x\\a.rs", 100));
    expect(parseFileRef("C:100")).toEqual(ref("C:100"));
  });

  it("leaves a non-numeric colon in the filename", () => {
    expect(parseFileRef("/tmp/a:b.txt")).toEqual(ref("/tmp/a:b.txt"));
  });

  it("treats :0 as no citation", () => {
    expect(parseFileRef("/a/b.ts:0")).toEqual(ref("/a/b.ts"));
  });

  it("ignores a number too large to be a line", () => {
    expect(parseFileRef("/a/b.ts:99999999")).toEqual(ref("/a/b.ts"));
  });

  it("handles an empty string", () => {
    expect(parseFileRef("")).toEqual(ref(""));
  });
});
