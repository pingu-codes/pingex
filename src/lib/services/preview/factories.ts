// Builders for the records the preview fixtures and the tests fill in by the
// dozen. Each starts from a plain, idle record and takes the fields that
// matter to the caller.

import type { DiffLine, InboxPr, PrComment, PrDetail, PrFile, ThreadSummary, WorktreeEntry } from "$lib/types";

/** The anchor fields of a comment GitHub gave no diff context for. */
export const NO_ANCHOR = {
  startLine: null,
  originalLine: null,
  originalStartLine: null,
  diffHunk: null,
  outdated: false,
} satisfies Partial<PrComment>;

/**
 * `detail` with each inline comment's `diffHunk` filled from its own diff, the
 * way GitHub sends it: the file's hunk cut off at the commented head line.
 */
export function withAnchors(detail: PrDetail): PrDetail {
  for (const comment of detail.comments) {
    const line = comment.originalLine ?? comment.line;
    if (comment.path == null || line == null || comment.diffHunk) continue;
    for (const hunk of detail.files.find((file) => file.path === comment.path)?.hunks ?? []) {
      const end = hunk.lines.findIndex((row) => row.kind !== "del" && row.newLine === line);
      if (end < 0) continue;
      comment.diffHunk = { ...hunk, lines: hunk.lines.slice(0, end + 1) };
      comment.originalLine = line;
      if (!comment.outdated) comment.originalStartLine ??= comment.startLine;
      break;
    }
  }
  return detail;
}

/** A clean linked worktree on branch `feature`. */
export function worktreeEntry(overrides: Partial<WorktreeEntry> = {}): WorktreeEntry {
  return {
    path: "/repo/wt-feature",
    head: "abcdef1",
    branch: "feature",
    detached: false,
    bare: false,
    locked: false,
    lockReason: null,
    prunable: false,
    prunableReason: null,
    isMain: false,
    isCodexManaged: false,
    isTemp: false,
    isManagedPrWorktree: false,
    missingDir: false,
    branchCheckedOutElsewhere: false,
    upstream: null,
    ahead: 0,
    behind: 0,
    status: { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 },
    state: null,
    ...overrides,
  };
}

/** An idle, unpinned top-level thread updated just now. */
export function threadSummary(overrides: Partial<ThreadSummary> & Pick<ThreadSummary, "id" | "title">): ThreadSummary {
  return {
    cwd: "",
    updatedAt: Date.now() / 1000,
    status: "idle",
    pinned: false,
    parentThreadId: null,
    agentNickname: null,
    agentRole: null,
    projectId: null,
    sectionId: null,
    subagentCount: 0,
    hidden: false,
    inTempWorktree: false,
    ...overrides,
  };
}

/** An open, green PR by `ciaran` on `feature` into `main`, nobody asked to review. */
export function inboxPr(overrides: Partial<InboxPr> & Pick<InboxPr, "number" | "title">): InboxPr {
  const sha = (seed: number) => String(seed).repeat(40).slice(0, 40);
  return {
    author: "ciaran",
    url: `https://github.com/ciaran/codex-custom/pull/${overrides.number}`,
    isDraft: false,
    createdAt: "2026-11-03T09:00:00Z",
    updatedAt: "2026-11-05T09:00:00Z",
    headRef: "feature",
    headSha: sha(overrides.number),
    baseRef: "main",
    baseSha: sha(1),
    headRepo: null,
    maintainerCanModify: false,
    reviewDecision: null,
    reviewRequests: [],
    reviews: [],
    checks: "SUCCESS",
    unresolvedThreads: 0,
    additions: 40,
    deletions: 8,
    changedFiles: 3,
    ...overrides,
  };
}

/** Changed files for a large diff: `files` files of `lines` diff lines each,
 *  one hunk per 50 lines, mixing context, added and removed lines, with
 *  every 7th line long enough to scroll sideways. */
export function largeDiffFiles(files: number, lines: number): PrFile[] {
  return Array.from({ length: files }, (_, fileIndex) => {
    const hunks = [];
    let oldLine = 1;
    let newLine = 1;
    for (let start = 0; start < lines; start += 50) {
      const hunkLines: DiffLine[] = [];
      const oldStart = oldLine;
      const newStart = newLine;
      for (let index = start; index < Math.min(start + 50, lines); index++) {
        const kind = index % 5 === 3 ? "add" : index % 5 === 4 ? "del" : "context";
        const content = `  const value${index} = compute(${index}, "${fileIndex}");${index % 7 === 0 ? " // long".padEnd(240, "-") : ""}`;
        hunkLines.push({
          kind,
          content,
          oldLine: kind === "add" ? null : oldLine++,
          newLine: kind === "del" ? null : newLine++,
        });
      }
      hunks.push({
        header: `@@ -${oldStart},${oldLine - oldStart} +${newStart},${newLine - newStart} @@`,
        oldStart,
        oldLines: oldLine - oldStart,
        newStart,
        newLines: newLine - newStart,
        lines: hunkLines,
      });
      // A gap between hunks, as in a real diff.
      oldLine += 10;
      newLine += 10;
    }
    const all = hunks.flatMap((hunk) => hunk.lines);
    return {
      path: `src/large/file-${String(fileIndex).padStart(2, "0")}.ts`,
      oldPath: null,
      status: "modified",
      additions: all.filter((line) => line.kind === "add").length,
      deletions: all.filter((line) => line.kind === "del").length,
      patch: "",
      hunks,
      patchTruncated: false,
      blobSha: null,
    };
  });
}
