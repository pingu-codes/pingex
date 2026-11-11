/** The review page's changed files as a directory tree. */

import type { PrFile } from "$lib/types";

/** A directory (with `children`) or a changed file (with `file`). A
 *  directory's id is its path; a run of directories that each hold only one
 *  directory is one node, named `a/b/c`, with the id of the deepest. */
export type FileTreeNode = { id: string; name: string; file?: PrFile; children?: FileTreeNode[] };

/** The root's id: no file or directory path can be this. */
export const FILE_TREE_ROOT = ".";

/** The files as a tree under a nameless root: directories before files, each
 *  by name. */
export function buildFileTree(files: PrFile[]): FileTreeNode {
  const root: FileTreeNode = { id: FILE_TREE_ROOT, name: "", children: [] };
  const dirs = new Map<string, FileTreeNode>();
  for (const file of files) {
    const parts = file.path.split("/");
    let parent = root;
    for (let depth = 1; depth < parts.length; depth++) {
      const id = parts.slice(0, depth).join("/");
      let dir = dirs.get(id);
      if (!dir) {
        dir = { id, name: parts[depth - 1], children: [] };
        dirs.set(id, dir);
        parent.children?.push(dir);
      }
      parent = dir;
    }
    parent.children?.push({ id: file.path, name: parts[parts.length - 1], file });
  }
  root.children = (root.children ?? []).map(compress);
  sortTree(root);
  return root;
}

function compress(node: FileTreeNode): FileTreeNode {
  if (!node.children) return node;
  let dir = node;
  while (dir.children?.length === 1 && dir.children[0].children) {
    const only = dir.children[0];
    dir = { id: only.id, name: `${dir.name}/${only.name}`, children: only.children };
  }
  return { ...dir, children: dir.children?.map(compress) };
}

function sortTree(node: FileTreeNode) {
  if (!node.children) return;
  node.children.sort(
    (a, b) => Number(Boolean(b.children)) - Number(Boolean(a.children)) || a.name.localeCompare(b.name),
  );
  for (const child of node.children) sortTree(child);
}

/** Every directory's id in the tree. */
export function directoryIds(node: FileTreeNode): string[] {
  const ids: string[] = [];
  const visit = (at: FileTreeNode) => {
    for (const child of at.children ?? []) {
      if (!child.children) continue;
      ids.push(child.id);
      visit(child);
    }
  };
  visit(node);
  return ids;
}

/** Every directory path above `path`, nearest last. Some may not be node ids
 *  (a compressed run keeps only the deepest), which callers can ignore. */
export function ancestorsOf(path: string): string[] {
  const parts = path.split("/");
  return parts.slice(0, -1).map((_, index) => parts.slice(0, index + 1).join("/"));
}
