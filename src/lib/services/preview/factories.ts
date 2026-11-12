// Builders for the records the preview fixtures and the tests fill in by the
// dozen. Each starts from a plain, idle record and takes the fields that
// matter to the caller. The desk builders at the end make a `RepositorySeed`
// for `seedPreviewDesk`, so a test states the repositories, PRs, threads and
// stacks it needs instead of reading the demo's.

import type {
  AgentReview,
  DiffLine,
  InboxPr,
  PrCheckout,
  PrComment,
  PrDetail,
  PrFile,
  PrSummary,
  Repository,
  ThreadSummary,
  WorktreeEntry,
} from "$lib/types";
import type { RepositorySeed, StackSeed } from "./desk";

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
    purpose: null,
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

/** A 40-character SHA made from `seed`, so each PR's head is its own. */
export const sha = (seed: number) => String(seed).repeat(40).slice(0, 40);

/** An open, green PR by `ciaran` on `feature` into `main`, nobody asked to review. */
export function inboxPr(overrides: Partial<InboxPr> & Pick<InboxPr, "number" | "title">): InboxPr {
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
    assignees: [],
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

// --- the review desk ---------------------------------------------------------

/** A clone at `path` whose `origin` is GitHub repository `slug`. */
export function clonedRepository(slug: string, path = `/repos/${slug.split("/").at(-1)}`): Repository {
  return {
    key: path,
    name: path.split("/").at(-1) ?? path,
    slug,
    path,
    state: "cloned",
    feeds: [{ remote: "origin", slug, onlyMine: false }],
  };
}

/**
 * A desk repository: `repository` (a clone of `slug` at `/repos/<name>` unless
 * given) with the PRs and stacks in `fields`. A stack's PRs join the list.
 */
export function deskRepository(
  slug: string,
  fields: Omit<RepositorySeed, "repository" | "stacks"> & {
    repository?: Partial<Repository>;
    stacks?: { prs: InboxPr[]; stack: StackSeed }[];
  } = {},
): RepositorySeed {
  const { repository, stacks = [], prs = [], ...rest } = fields;
  return {
    repository: { ...clonedRepository(slug, repository?.path ?? undefined), ...repository },
    prs: [...prs, ...stacks.flatMap((entry) => entry.prs)],
    stacks: stacks.map((entry) => entry.stack),
    ...rest,
  };
}

/**
 * Stack `number`: `prs` bottom first, each based on the head of the one
 * below and the bottom on `baseRef`. `sim` sets how the stack fakes answer:
 * how far behind its base it is, its worktree, a conflict to stop at.
 */
export function prStack(
  number: number,
  prs: InboxPr[],
  sim: Omit<StackSeed, "number" | "prs"> = {},
): { prs: InboxPr[]; stack: StackSeed } {
  const baseRef = sim.baseRef ?? "main";
  const chained = prs.map((pr, index) =>
    index === 0 ? { ...pr, baseRef } : { ...pr, baseRef: prs[index - 1].headRef, baseSha: prs[index - 1].headSha },
  );
  return { prs: chained, stack: { ...sim, number, baseRef, prs: prs.map((pr) => pr.number) } };
}

/** The open PR `pr` as the PR list summarizes it. */
export function prSummary(pr: InboxPr): PrSummary {
  return {
    number: pr.number,
    title: pr.title,
    author: pr.author,
    state: "OPEN",
    isDraft: pr.isDraft,
    baseRef: pr.baseRef,
    headRef: pr.headRef,
    updatedAt: pr.updatedAt,
    url: pr.url,
    headRepo: pr.headRepo,
    maintainerCanModify: pr.maintainerCanModify,
  };
}

/** The detail page of `pr`: its summary, no commits, and `files`, `comments`
 *  and `reviews` as given (each inline comment anchored on its own diff). */
export function prDetail(pr: InboxPr, overrides: Partial<PrDetail> = {}): PrDetail {
  return withAnchors({
    summary: prSummary(pr),
    body: "",
    headSha: pr.headSha,
    baseSha: pr.baseSha,
    commits: [],
    files: [],
    comments: [],
    reviews: [],
    checks: null,
    filesTruncated: false,
    ...structuredClone(overrides),
  });
}

/** A new file at `path` whose lines are `lines`, all added. */
export function addedFile(path: string, lines: string[]): PrFile {
  const header = `@@ -0,0 +1,${lines.length} @@`;
  return {
    path,
    oldPath: null,
    status: "added",
    additions: lines.length,
    deletions: 0,
    patch: [header, ...lines.map((line) => `+${line}`)].join("\n"),
    patchTruncated: false,
    blobSha: `b10b${path.length}`.padEnd(40, "0"),
    hunks: [
      {
        header,
        oldStart: 0,
        oldLines: 0,
        newStart: 1,
        newLines: lines.length,
        lines: lines.map((content, index) => ({ kind: "add", content, oldLine: null, newLine: index + 1 })),
      },
    ],
  };
}

/**
 * A review thread on `path` at `line`: one comment per message, the first
 * posted in review `review`, each reply in one of its own. Comment ids are the
 * thread id's digits times ten, plus the message's index.
 */
export function reviewThread(
  threadId: string,
  path: string,
  line: number,
  messages: [author: string, body: string][],
  options: { resolved?: boolean; review?: string; anchor?: Partial<PrComment> } = {},
): PrComment[] {
  return messages.map(([author, body], index) => ({
    id: Number.parseInt(threadId.replace(/\D/g, ""), 10) * 10 + index,
    author,
    body,
    createdAt: `2026-11-04T1${index}:00:00Z`,
    path,
    line,
    side: "RIGHT",
    threadId,
    isResolved: options.resolved ?? false,
    reviewId: index === 0 ? (options.review ?? `${threadId}_review`) : `${threadId}_reply${index}`,
    ...NO_ANCHOR,
    ...options.anchor,
  }));
}

/** PR `number` checked out in its own worktree, or where `overrides` puts it. */
export function prCheckout(number: number, overrides: Partial<PrCheckout> = {}): PrCheckout {
  return {
    number,
    path: `/wt/pr-${number}`,
    headSha: null,
    gitBranch: `pr/${number}`,
    setupPending: null,
    ahead: 0,
    behind: 0,
    dirty: false,
    adopted: false,
    onPrBranch: true,
    mainCheckout: false,
    stack: null,
    ...overrides,
  };
}

/** A queued diff review of `pr` by Claude, in the repository at `repoDir`. */
export function agentReview(
  repoDir: string,
  repo: string,
  pr: InboxPr,
  overrides: Partial<AgentReview> & Pick<AgentReview, "id">,
): AgentReview {
  return {
    repoDir,
    repo,
    number: pr.number,
    request: {
      targets: [{ number: pr.number, headRef: pr.headRef, headSha: pr.headSha, mine: false, stack: null }],
      harness: "claude",
      focus: ["correctness", "tests"],
      keepDrafts: true,
      rerunOnNewCommits: false,
    },
    status: "queued",
    threadId: null,
    headSha: pr.headSha,
    sinceSha: null,
    findings: 0,
    parseError: null,
    error: null,
    createdAt: 0,
    startedAt: null,
    finishedAt: null,
    ...overrides,
  };
}
