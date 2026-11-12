// Pure helpers for the one-page review (`ReviewPage.svelte`): the rows a
// file's diff renders, line ranges, the file tree's counts, where the page was
// left, Up next, and Ask agent. Reading the page of diffs (the current file,
// folds, selection, keys) is `diffReading.svelte.ts`'s.

import { place } from "$lib/app/places";
import type { CommentThread } from "$lib/review/review";
import type { DiffAnchor, DiffLine, FileBlob, Inbox, InboxPr, PendingEntry, PrFile } from "$lib/types";
import { basename } from "$lib/utils/path";

/** `RIGHT` is the head, `LEFT` the base. */
export type DiffSide = "LEFT" | "RIGHT";

export type DiffRow =
  | { kind: "hunk"; key: string; header: string }
  | { kind: "line"; key: string; line: DiffLine; side: DiffSide; number: number };

/** Every hunk header and line of `file`, each line on the side a comment on
 *  it anchors to: removed lines on the base, the rest on the head. */
export function diffRows(file: PrFile): DiffRow[] {
  const rows: DiffRow[] = [];
  file.hunks.forEach((hunk, hunkIndex) => {
    rows.push({ kind: "hunk", key: `h${hunkIndex}`, header: hunk.header });
    hunk.lines.forEach((line, lineIndex) => {
      const side: DiffSide = line.kind === "del" ? "LEFT" : "RIGHT";
      const number = side === "LEFT" ? line.oldLine : line.newLine;
      if (number == null) return;
      rows.push({ kind: "line", key: `h${hunkIndex}l${lineIndex}`, line, side, number });
    });
  });
  return rows;
}

/** The line (`side:line`) a thread sits under. */
export const threadLineKey = (thread: CommentThread) => `${thread.side ?? "RIGHT"}:${thread.line}`;

/** A run of a file's diff rows that mounts as one (`DiffChunk.svelte`). */
export type RowChunk = { key: string; rows: DiffRow[]; columns: number };

/** Rows per chunk: about a screen. Small enough to mount within a frame. */
export const CHUNK_ROWS = 60;

/** `rows` in runs of `size`, each with its longest line in columns (a tab
 *  counted as 8). */
export function chunkRows(rows: DiffRow[], size = CHUNK_ROWS): RowChunk[] {
  const chunks: RowChunk[] = [];
  for (let start = 0; start < rows.length; start += size) {
    const slice = rows.slice(start, start + size);
    let columns = 0;
    for (const row of slice) {
      const text = row.kind === "hunk" ? row.header : row.line.content;
      let width = text.length;
      for (let at = text.indexOf("\t"); at >= 0; at = text.indexOf("\t", at + 1)) width += 7;
      columns = Math.max(columns, width);
    }
    chunks.push({ key: slice[0].key, rows: slice, columns });
  }
  return chunks;
}

/** `next`, keeping `previous`'s value for each key whose value is equal by
 *  `same`: a file whose threads or pending entries did not change keeps the
 *  same array, so its rows don't re-render when another file's change. */
export function reuseUnchanged<T>(
  previous: Map<string, T>,
  next: Map<string, T>,
  same: (a: T, b: T) => boolean,
): Map<string, T> {
  for (const [key, value] of next) {
    const old = previous.get(key);
    if (old !== undefined && old !== value && same(old, value)) next.set(key, old);
  }
  return next;
}

/** Same length and the same items in order. */
export function sameItems<T>(a: readonly T[], b: readonly T[]): boolean {
  return a.length === b.length && a.every((item, index) => item === b[index]);
}

/** Same keys with the same values. */
export function sameFields<T extends object>(a: T, b: T): boolean {
  const keys = Object.keys(a) as (keyof T)[];
  return keys.length === Object.keys(b).length && keys.every((key) => a[key] === b[key]);
}

/** Group items by key, keeping their order within each group: what sits
 *  under each diff line, looked up once per row. */
export function groupBy<T>(items: T[], key: (item: T) => string): Map<string, T[]> {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const at = key(item);
    const group = groups.get(at);
    if (group) group.push(item);
    else groups.set(at, [item]);
  }
  return groups;
}

/** Selected lines: `anchor` is where the selection started, `start..=end`
 *  the range, all on one side of one file. */
export type LineRange = { path: string; side: DiffSide; anchor: number; start: number; end: number };

export function inRange(range: LineRange | null, path: string, side: DiffSide, number: number): boolean {
  return !!range && range.path === path && range.side === side && number >= range.start && number <= range.end;
}

/** "Line 7" or "Lines 3–5". */
export function rangeLabel(range: { start: number; end: number }): string {
  return range.start === range.end ? `Line ${range.start}` : `Lines ${range.start}–${range.end}`;
}

export function rangeAnchor(range: LineRange): DiffAnchor {
  return {
    path: range.path,
    side: range.side,
    line: range.end,
    startLine: range.start < range.end ? range.start : null,
  };
}

/** The selected lines' text, for a suggestion to start from. */
export function rangeText(file: PrFile, range: LineRange): string {
  return diffRows(file)
    .filter((row) => row.kind === "line" && inRange(range, file.path, row.side, row.number))
    .map((row) => (row.kind === "line" ? row.line.content : ""))
    .join("\n");
}

/** "loader.ts:3–5" or "loader.ts:7". */
export function anchorLabel(path: string, line: number | null, startLine?: number | null): string {
  if (line == null) return basename(path);
  return startLine != null && startLine < line ? `${basename(path)}:${startLine}–${line}` : `${basename(path)}:${line}`;
}

/** Where a pending entry sits: its file and line, for the inline card and
 *  the Your review list. */
export function entryLocation(entry: PendingEntry): { path: string; side: DiffSide; line: number | null } {
  if (entry.kind === "reply") return { path: entry.path, side: "RIGHT", line: entry.line };
  return { path: entry.anchor.path, side: entry.anchor.side as DiffSide, line: entry.anchor.line };
}

export function entryLabel(entry: PendingEntry): string {
  if (entry.kind === "reply") return anchorLabel(entry.path, entry.line);
  return anchorLabel(entry.anchor.path, entry.anchor.line, entry.anchor.startLine);
}

// --- the file tree -----------------------------------------------------------

/** What the file tree and a file's header count. `findings` are agent
 *  Findings, which agent reviews fill in. */
export type FileCounts = { comments: number; unresolved: number; findings: number };

export function fileCounts(threads: CommentThread[], path: string, findings = 0): FileCounts {
  const own = threads.filter((thread) => thread.path === path);
  return {
    comments: own.reduce((sum, thread) => sum + thread.comments.length, 0),
    unresolved: own.filter((thread) => !thread.resolved).length,
    findings,
  };
}

/** A file's viewed key: its blob id at the head, or a hash of its patch when
 *  the provider gave none (a local diff), so a changed file still resets. */
export function fileBlob(file: PrFile): FileBlob {
  if (file.blobSha) return { path: file.path, blobSha: file.blobSha };
  let hash = 5381;
  for (let index = 0; index < file.patch.length; index++) {
    hash = ((hash << 5) + hash + file.patch.charCodeAt(index)) | 0;
  }
  return { path: file.path, blobSha: `patch:${(hash >>> 0).toString(16)}` };
}

// --- filters ---------------------------------------------------------------

/** The Files tab's comment filter. Agent reviews add `findings`. */
export type ReviewFilter = "all" | "unresolved" | "findings";

export function threadShown(thread: CommentThread, filter: ReviewFilter): boolean {
  if (filter === "unresolved") return !thread.resolved;
  if (filter === "findings") return false;
  return true;
}

// --- what the page had open ---------------------------------------------------

export type ReviewTab = "conversation" | "files" | "checks" | "threads";

/** A PR's page as it was left, and how it next opens: asking for `changes:
 *  "since"` here opens it on the changes since the reviewer's last review. */
export function prPlace(repoDir: string, number: number) {
  return place(`pr:${repoDir}#${number}`, () => ({
    tab: "files" as ReviewTab,
    filter: "all" as ReviewFilter,
    changes: "all" as "all" | "since",
    scrollTop: 0,
    /** `null` until the page has read the viewed marks, which collapse their files. */
    collapsed: null as string[] | null,
    /** The file tree's folded directories. */
    collapsedDirs: [] as string[],
  }));
}

// --- the conversation and Up next -------------------------------------------

/** PRs asking the viewer for review (re-requests included, as the inbox
 *  triages them), other than `current`, lowest number first. */
export function upNext(inbox: Inbox | null, current: number): InboxPr[] {
  if (!inbox) return [];
  const requested = new Set(inbox.triage.filter((pr) => pr.reviewRequested).map((pr) => pr.number));
  return inbox.prs
    .filter((pr) => pr.number !== current && (requested.has(pr.number) || pr.reviewRequests.includes(inbox.viewer)))
    .sort((a, b) => a.number - b.number);
}

/** The PR `step` places from `current` in the review queue (`current` plus
 *  Up next, by number): `1` is the next one, `-1` the previous. */
export function queueNeighbour(queue: InboxPr[], current: number, step: 1 | -1): InboxPr | null {
  const numbers = [...new Set([...queue.map((pr) => pr.number), current])].sort((a, b) => a - b);
  const at = numbers.indexOf(current) + step;
  return queue.find((pr) => pr.number === numbers[at]) ?? null;
}

// --- Ask agent ---------------------------------------------------------------

/** What "Ask agent" hands over: lines, a whole file, or one comment. It is
 *  answered in a thread filed under the PR, new or existing; the message is
 *  `askAgentPrompt`. */
export type AskAgentRequest = {
  number: number;
  title: string;
  path: string;
  side: DiffSide | null;
  startLine: number | null;
  line: number | null;
  /** The selected lines, or the comment's text. */
  text: string;
  /** "Ask only, no edits". */
  askOnly: boolean;
  /** Send it to this thread on the PR rather than a new one. */
  threadId?: string | null;
};

export function askAgentPrompt(request: AskAgentRequest): string {
  const where =
    request.line == null
      ? request.path
      : `${request.path}:${request.startLine != null && request.startLine < request.line ? `${request.startLine}-` : ""}${request.line}`;
  const lines = [
    `About pull request #${request.number} "${request.title}", at ${where}:`,
    "",
    "```",
    request.text,
    "```",
    "",
  ];
  if (request.askOnly) lines.push("Answer only. Do not edit any files.");
  return lines.join("\n");
}
