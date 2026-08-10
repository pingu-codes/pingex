import type { ChecksSummary, FileUpdateChange, PrComment, PrFile, PrFreshness } from "$lib/types";

/**
 * Map a changed PR file to the `FileUpdateChange` shape `DiffBlock` renders.
 * The review view reuses the existing diff renderer rather than forking it; the
 * hunks (with their line anchors) drive the separate "add comment" affordance.
 */
export function fileChange(file: PrFile): FileUpdateChange {
  const type =
    file.status === "added"
      ? "add"
      : file.status === "removed"
        ? "delete"
        : file.status === "renamed"
          ? "rename"
          : "update";
  return {
    path: file.path,
    kind: { type, movePath: file.oldPath ?? null },
    diff: file.patch,
  };
}

/** A compact per-file change stat like "+5 −2". */
export function changeStat(file: PrFile): string {
  const parts: string[] = [];
  if (file.additions > 0) parts.push(`+${file.additions}`);
  if (file.deletions > 0) parts.push(`−${file.deletions}`);
  return parts.length > 0 ? parts.join(" ") : "no change";
}

/** A stable anchor for one file's diff (used for scroll/selection). */
export function fileAnchor(file: PrFile): string {
  return `file:${file.path}`;
}

/** A stable anchor for one line of a diff, keyed by side and line number. */
export function lineAnchor(path: string, side: string, line: number): string {
  return `line:${path}:${side}:${line}`;
}

export interface AddableLine {
  side: string;
  line: number;
  content: string;
  /** A short label for a line picker, e.g. "+7  return readFileSync(path);". */
  label: string;
  anchor: string;
}

/**
 * The lines of a file a reviewer can attach an inline comment to: added and
 * context lines anchor to the RIGHT (head) side; removed lines to the LEFT
 * (base) side. Uses the parsed hunk anchors so a comment lands on an exact line.
 */
export function addableLines(file: PrFile): AddableLine[] {
  const lines: AddableLine[] = [];
  for (const hunk of file.hunks) {
    for (const diffLine of hunk.lines) {
      const onRight = diffLine.kind !== "del" && diffLine.newLine != null;
      const side = onRight ? "RIGHT" : "LEFT";
      const line = onRight ? diffLine.newLine : diffLine.oldLine;
      if (line == null) continue;
      const marker = diffLine.kind === "add" ? "+" : diffLine.kind === "del" ? "−" : " ";
      lines.push({
        side,
        line,
        content: diffLine.content,
        label: `${marker}${line}  ${diffLine.content}`.slice(0, 80),
        anchor: lineAnchor(file.path, side, line),
      });
    }
  }
  return lines;
}

export interface CommentThread {
  /** Thread node id, or a synthesized "path:line" key when GraphQL gave none. */
  key: string;
  path: string;
  line: number | null;
  side: string | null;
  resolved: boolean;
  comments: PrComment[];
}

/**
 * Group inline (path-anchored) comments into threads. GitHub review threads
 * share a `threadId`; comments lacking one are grouped by path+line so replies
 * still cluster. Conversation comments (no path) are excluded — see
 * `conversationComments`.
 */
export function commentThreads(comments: PrComment[]): CommentThread[] {
  const byKey = new Map<string, CommentThread>();
  for (const comment of comments) {
    if (comment.path == null) continue;
    const key = comment.threadId ?? `${comment.path}:${comment.line ?? 0}:${comment.side ?? "RIGHT"}`;
    let thread = byKey.get(key);
    if (!thread) {
      thread = {
        key,
        path: comment.path,
        line: comment.line,
        side: comment.side,
        resolved: comment.isResolved,
        comments: [],
      };
      byKey.set(key, thread);
    }
    // Any resolved marker on a comment marks the whole thread resolved.
    thread.resolved = thread.resolved || comment.isResolved;
    thread.comments.push(comment);
  }
  return [...byKey.values()];
}

/** Inline threads for a specific file, in stable line order. */
export function threadsForFile(comments: PrComment[], path: string): CommentThread[] {
  return commentThreads(comments)
    .filter((thread) => thread.path === path)
    .sort((a, b) => (a.line ?? 0) - (b.line ?? 0));
}

/** Conversation (non-inline) comments, oldest first. */
export function conversationComments(comments: PrComment[]): PrComment[] {
  return comments.filter((comment) => comment.path == null);
}

/** A short checks label like "3 passing · 1 pending" or null when no checks. */
export function checksLabel(checks: ChecksSummary | null): string | null {
  if (!checks || checks.total === 0) return null;
  const parts: string[] = [];
  if (checks.passing > 0) parts.push(`${checks.passing} passing`);
  if (checks.failing > 0) parts.push(`${checks.failing} failing`);
  if (checks.pending > 0) parts.push(`${checks.pending} pending`);
  return parts.join(" · ");
}

/** Whether the checks rollup should read as a failure. */
export function checksFailing(checks: ChecksSummary | null): boolean {
  return !!checks && checks.failing > 0;
}

/**
 * The stale-data banner text, or null when the open PR still matches the remote.
 * Drift in either the head SHA or the updated timestamp counts as stale.
 */
export function staleBanner(freshness: PrFreshness | null): string | null {
  if (!freshness?.stale) return null;
  const head = freshness.remoteHead ? freshness.remoteHead.slice(0, 7) : "unknown";
  return `This pull request changed on the remote (now at ${head}). Refresh to see the latest.`;
}

/** Human label for a submitted review event. */
export function reviewEventLabel(event: string): string {
  switch (event) {
    case "approve":
      return "Approve";
    case "request-changes":
      return "Request changes";
    default:
      return "Comment";
  }
}

/**
 * Build the prompt handed to Codex when asking for a review, embedding the PR
 * title, branches, and per-file diffs so the agent has the full context.
 */
export function reviewPrompt(title: string, baseRef: string, headRef: string, files: PrFile[]): string {
  const header = [
    `Please review this pull request and point out bugs, risks, and improvements.`,
    ``,
    `Title: ${title}`,
    `Branch: ${headRef} → ${baseRef}`,
    `Changed files: ${files.length}`,
    ``,
  ];
  const diffs = files
    .filter((file) => !file.patchTruncated && file.patch)
    .map((file) => `--- ${file.path} (${changeStat(file)}) ---\n${file.patch}`);
  return [...header, ...diffs].join("\n");
}
