import type { StatusFile } from "$lib/types";

/**
 * `git status` reports one entry per path with a two-letter XY code: X is the
 * index side, Y the working tree. A path can therefore be both staged and
 * unstaged (`MM`), and then appears in both sections.
 */
export type StatusSection = "conflicted" | "staged" | "unstaged" | "untracked";

export interface StatusRow {
  path: string;
  section: StatusSection;
  /** Single letter describing the change on this side: M, A, D, R, ?, U. */
  change: string;
  file: StatusFile;
}

export interface StatusGroups {
  conflicted: StatusRow[];
  staged: StatusRow[];
  unstaged: StatusRow[];
  untracked: StatusRow[];
}

export function isStagedCode(code: string): boolean {
  return code.length >= 1 && code[0] !== ".";
}

export function isUnstagedCode(code: string): boolean {
  return code.length >= 2 && code[1] !== ".";
}

export function groupStatusFiles(files: StatusFile[]): StatusGroups {
  const groups: StatusGroups = { conflicted: [], staged: [], unstaged: [], untracked: [] };
  for (const file of files) {
    if (file.state === "ignored") continue;
    if (file.state === "untracked") {
      groups.untracked.push({ path: file.path, section: "untracked", change: "?", file });
      continue;
    }
    if (file.state === "conflicted") {
      groups.conflicted.push({ path: file.path, section: "conflicted", change: "U", file });
      continue;
    }
    if (isStagedCode(file.code)) groups.staged.push({ path: file.path, section: "staged", change: file.code[0], file });
    if (isUnstagedCode(file.code))
      groups.unstaged.push({ path: file.path, section: "unstaged", change: file.code[1], file });
  }
  return groups;
}

export interface Selection {
  path: string;
  section: StatusSection;
}

/**
 * Keep the selected row across a refresh when it still exists; otherwise
 * fall back to the same path in another section (a staged file that was
 * unstaged), else nothing.
 */
export function selectionAfterRefresh(previous: Selection | null, groups: StatusGroups): Selection | null {
  if (!previous) return null;
  const rows = [...groups.conflicted, ...groups.staged, ...groups.unstaged, ...groups.untracked];
  if (rows.some((row) => row.path === previous.path && row.section === previous.section)) return previous;
  const samePath = rows.find((row) => row.path === previous.path);
  return samePath ? { path: samePath.path, section: samePath.section } : null;
}

export const changeLabel: Record<string, string> = {
  M: "Modified",
  A: "Added",
  D: "Deleted",
  R: "Renamed",
  C: "Copied",
  T: "Type changed",
  U: "Conflict",
  "?": "Untracked",
};
