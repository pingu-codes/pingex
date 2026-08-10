/**
 * What `/review` offers to review, mirroring the Codex TUI's picker: the
 * working tree, a base branch, or a single commit. The pure parts live here so
 * the picker component only has to draw rows.
 */

import type { GitBranch, GitCommit } from "$lib/types";

/** The picker's first stage; choosing one either reviews or opens a list. */
export type ReviewMode = "uncommittedChanges" | "baseBranch" | "commit";

export interface ReviewModeOption {
  id: ReviewMode;
  label: string;
  /** Shown once the list this mode opens is on screen. */
  listLabel: string;
  /** Placeholder for that list when it comes back empty. */
  emptyMessage: string;
}

export const REVIEW_MODES: ReviewModeOption[] = [
  {
    id: "uncommittedChanges",
    label: "Review uncommitted changes",
    listLabel: "Review targets",
    emptyMessage: "No review targets.",
  },
  {
    id: "baseBranch",
    label: "Review against a base branch",
    listLabel: "Base branches",
    emptyMessage: "No branches in this repository.",
  },
  {
    id: "commit",
    label: "Review a commit",
    listLabel: "Commits",
    emptyMessage: "No commits in this repository.",
  },
];

/** Matching modes for the text typed after `/review` opened the picker. */
export function filterModes(query: string): ReviewModeOption[] {
  const lowered = query.trim().toLowerCase();
  if (!lowered) return REVIEW_MODES;
  return REVIEW_MODES.filter((mode) => mode.label.toLowerCase().includes(lowered));
}

/**
 * Branches matching the query, best matches first: the branch name's own
 * segment before the remote or namespace prefix, so `main` finds `main` ahead
 * of `origin/main` and `feature/maintenance`.
 */
export function filterBranches(branches: GitBranch[], query: string): GitBranch[] {
  const lowered = query.trim().toLowerCase();
  if (!lowered) return branches;
  const prefix: GitBranch[] = [];
  const elsewhere: GitBranch[] = [];
  for (const branch of branches) {
    const name = branch.name.toLowerCase();
    if (name.startsWith(lowered)) prefix.push(branch);
    else if (name.includes(lowered)) elsewhere.push(branch);
  }
  return [...prefix, ...elsewhere];
}

/** Commits whose subject or hash matches the query, in the order given. */
export function filterCommits(commits: GitCommit[], query: string): GitCommit[] {
  const lowered = query.trim().toLowerCase();
  if (!lowered) return commits;
  return commits.filter(
    (commit) => commit.subject.toLowerCase().includes(lowered) || commit.hash.toLowerCase().startsWith(lowered),
  );
}
