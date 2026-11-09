// The few diff lines a review thread is about, shown above it away from the
// diff (the Conversation tab), the way GitHub does.
//
// GitHub's `diffHunk` ends at the commented line and is numbered against the
// commit the comment was made on, so rows are picked by `originalLine` and
// `originalStartLine`. The head `line` only names the place in the label.

import { highlightedRows } from "$lib/review/diffHighlight";
import type { CommentThread } from "$lib/review/review";
import type { DiffHunk, DiffLine, PrFile } from "$lib/types";

/** Rows shown for a single-line comment, ending at the commented one. */
export const CONTEXT_ROWS = 4;
/** No snippet grows past this, however long the commented range. */
export const MAX_ROWS = 12;

export interface ContextRow {
  key: string;
  line: DiffLine;
  /** Inside the commented line or range. */
  commented: boolean;
  /** Highlighted HTML for the code, or `null` to show it as text. */
  html: string | null;
}

type Anchor = Pick<CommentThread, "path" | "side" | "originalLine" | "originalStartLine" | "diffHunk">;

/** The line number `line` has on `side`, or `null` when it is not on it. */
function numberOn(line: DiffLine, side: string): number | null {
  if (side === "LEFT") return line.kind === "add" ? null : line.oldLine;
  return line.kind === "del" ? null : line.newLine;
}

/**
 * The rows to show above `thread`: its commented range when it has one, else
 * the commented line with up to three lines before it. `null` when GitHub
 * gave no hunk, so the thread shows only where it is.
 */
export function contextRows(thread: Anchor): ContextRow[] | null {
  const hunk = thread.diffHunk;
  if (!hunk || hunk.lines.length === 0) return null;
  const side = thread.side ?? "RIGHT";
  const lines = hunk.lines;

  // The hunk ends on the commented line; search back in case it does not.
  let end = lines.length - 1;
  if (thread.originalLine != null) {
    while (end > 0 && numberOn(lines[end], side) !== thread.originalLine) end--;
    if (numberOn(lines[end], side) !== thread.originalLine) end = lines.length - 1;
  }
  let commentedFrom = end;
  const start = thread.originalStartLine;
  if (start != null && thread.originalLine != null && start < thread.originalLine) {
    const found = lines.findIndex((line, index) => index <= end && numberOn(line, side) === start);
    if (found >= 0) commentedFrom = found;
  }
  const from = Math.max(0, end - MAX_ROWS + 1, Math.min(commentedFrom, end - CONTEXT_ROWS + 1));

  const highlighted = highlight(thread.path, hunk);
  return lines.slice(from, end + 1).map((line, offset) => {
    const index = from + offset;
    const key = `h0l${index}`;
    return { key, line, commented: index >= commentedFrom, html: highlighted?.get(key) ?? null };
  });
}

/** A one-hunk stand-in file per hunk, so `highlightedRows` memoises it. */
const asFile = new WeakMap<DiffHunk, PrFile>();

function highlight(path: string, hunk: DiffHunk): Map<string, string> | null {
  let file = asFile.get(hunk);
  if (file?.path !== path) {
    file = {
      path,
      oldPath: null,
      status: "modified",
      additions: 0,
      deletions: 0,
      patch: "",
      hunks: [hunk],
      patchTruncated: false,
      blobSha: null,
    };
    asFile.set(hunk, file);
  }
  return highlightedRows(file);
}

/**
 * Where a thread sits, as `path:line` or `path:start–end`. An outdated
 * thread names the lines it was left on, which is what its snippet shows.
 */
export function threadLocation(thread: CommentThread): string {
  const line = thread.outdated ? (thread.originalLine ?? thread.line) : thread.line;
  const start = thread.outdated ? thread.originalStartLine : thread.startLine;
  if (line == null) return `${thread.path}:file`;
  return start != null && start < line ? `${thread.path}:${start}–${line}` : `${thread.path}:${line}`;
}
