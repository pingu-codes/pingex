import { describe, expect, it } from "vitest";
import { ancestorsOf, buildFileTree, directoryIds, type FileTreeNode } from "$lib/review/fileTree";
import type { PrFile } from "$lib/types";

const file = (path: string): PrFile => ({
  path,
  oldPath: null,
  status: "modified",
  additions: 1,
  deletions: 0,
  patch: "",
  hunks: [],
  patchTruncated: false,
  blobSha: null,
});

/** The tree as indented lines: a directory ends in a slash. */
function outline(node: FileTreeNode, depth = 0): string[] {
  return (node.children ?? []).flatMap((child) => [
    `${"  ".repeat(depth)}${child.name}${child.children ? "/" : ""}`,
    ...outline(child, depth + 1),
  ]);
}

describe("buildFileTree", () => {
  it("nests files under their directories, directories first, each by name", () => {
    const tree = buildFileTree([
      file("README.md"),
      file("src/b.ts"),
      file("src/lib/x.ts"),
      file("src/a.ts"),
      file("docs/guide.md"),
      file("docs/api/ref.md"),
    ]);
    expect(outline(tree)).toEqual([
      "docs/",
      "  api/",
      "    ref.md",
      "  guide.md",
      "src/",
      "  lib/",
      "    x.ts",
      "  a.ts",
      "  b.ts",
      "README.md",
    ]);
  });

  it("folds a run of single-directory directories into one node with the deepest id", () => {
    const tree = buildFileTree([file("src/lib/review/a.ts"), file("src/lib/review/b.ts")]);
    expect(outline(tree)).toEqual(["src/lib/review/", "  a.ts", "  b.ts"]);
    expect(tree.children?.[0].id).toBe("src/lib/review");
  });

  it("folds only where a directory holds nothing but one directory", () => {
    const tree = buildFileTree([file("a/b/c/one.ts"), file("a/b/two.ts")]);
    expect(outline(tree)).toEqual(["a/b/", "  c/", "    one.ts", "  two.ts"]);
    expect(directoryIds(tree)).toEqual(["a/b", "a/b/c"]);
  });

  it("keeps each file on its node and gives files their path as id", () => {
    const files = [file("top.ts"), file("dir/inner.ts")];
    const tree = buildFileTree(files);
    const inner = tree.children?.[0].children?.[0];
    expect(inner?.id).toBe("dir/inner.ts");
    expect(inner?.file).toBe(files[1]);
    expect(tree.children?.[1].file).toBe(files[0]);
  });

  it("is empty for no files", () => {
    expect(buildFileTree([]).children).toEqual([]);
  });
});

describe("ancestorsOf", () => {
  it("lists every directory above a path, outermost first", () => {
    expect(ancestorsOf("src/lib/a.ts")).toEqual(["src", "src/lib"]);
    expect(ancestorsOf("a.ts")).toEqual([]);
  });
});
