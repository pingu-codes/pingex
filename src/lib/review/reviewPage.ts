// Pure helpers for the one-page review (`ReviewPage.svelte`): the rows a
// file's diff renders, line-range selection, the file tree and its counts,
// "N new since", Up next, and the keyboard map.

import type { CommentThread } from "$lib/review/review";
import type { DiffAnchor, DiffLine, FileBlob, Inbox, InboxPr, PendingEntry, PrComment, PrFile } from "$lib/types";

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

/** Selected lines: `anchor` is where the selection started, `start..=end`
 *  the range, all on one side of one file. */
export type LineRange = { path: string; side: DiffSide; anchor: number; start: number; end: number };

/** Select one line, or with `extend` grow the current selection to it. A
 *  selection never spans files or sides; extending across starts afresh. */
export function selectLine(
  current: LineRange | null,
  path: string,
  side: DiffSide,
  number: number,
  extend: boolean,
): LineRange {
  if (extend && current && current.path === path && current.side === side) {
    return {
      ...current,
      start: Math.min(current.anchor, number),
      end: Math.max(current.anchor, number),
    };
  }
  return { path, side, anchor: number, start: number, end: number };
}

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

const basename = (path: string) => path.slice(path.lastIndexOf("/") + 1);

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

/** Changed files grouped by folder, folders in the order files first name
 *  them. */
export function fileGroups(files: PrFile[]): { dir: string; files: PrFile[] }[] {
  const groups = new Map<string, PrFile[]>();
  for (const file of files) {
    const slash = file.path.lastIndexOf("/");
    const dir = slash < 0 ? "" : file.path.slice(0, slash);
    groups.set(dir, [...(groups.get(dir) ?? []), file]);
  }
  return [...groups].map(([dir, files]) => ({ dir, files }));
}

export function fileName(path: string): string {
  return basename(path);
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

// --- the conversation and Up next -------------------------------------------

/** Where "N new since" goes in `comments` (oldest first): the first comment
 *  after `seenAt`, and how many follow. `null` when nothing is new or the
 *  reviewer has never looked. */
export function newSince(comments: PrComment[], seenAt: string | null): { index: number; count: number } | null {
  if (!seenAt) return null;
  const seen = Date.parse(seenAt);
  const index = comments.findIndex((comment) => Date.parse(comment.createdAt) > seen);
  return index < 0 ? null : { index, count: comments.length - index };
}

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

/** Initials for an avatar: "mira-k" → "MK", "tomasz" → "TO". */
export function initials(login: string): string {
  const parts = login.split(/[-_.\s]+/).filter(Boolean);
  const letters = parts.length > 1 ? parts[0][0] + parts[1][0] : login.slice(0, 2);
  return letters.toUpperCase();
}

// --- the keyboard ------------------------------------------------------------

/** A page-level review command a key runs. */
export type ReviewKeyCommand =
  | "nextPr"
  | "previousPr"
  | "nextFile"
  | "previousFile"
  | "nextUnresolved"
  | "toggleViewed"
  | "comment"
  | "submit"
  | "help";

/** The review page's shortcuts, as the Shortcuts card lists them. */
export const REVIEW_SHORTCUTS: { keys: string[]; label: string }[] = [
  { keys: ["j", "k"], label: "Next / previous PR" },
  { keys: ["]", "["], label: "Next / previous file" },
  { keys: ["n"], label: "Next unresolved thread" },
  { keys: ["v"], label: "Viewed and collapse" },
  { keys: ["c"], label: "Comment on selected lines" },
  { keys: ["⌘↵"], label: "Submit review" },
  { keys: ["?"], label: "Shortcuts" },
];

/** The command `event` presses, or `null`. Keys typed into a field are the
 *  field's, except ⌘↵ which submits from anywhere. */
export function reviewKey(event: KeyboardEvent): ReviewKeyCommand | null {
  if ((event.metaKey || event.ctrlKey) && event.key === "Enter") return "submit";
  if (event.metaKey || event.ctrlKey || event.altKey) return null;
  const target = event.target as HTMLElement | null;
  if (target?.closest?.("input, textarea, select, [contenteditable='true']")) return null;
  const map: Record<string, ReviewKeyCommand> = {
    j: "nextPr",
    k: "previousPr",
    "]": "nextFile",
    "[": "previousFile",
    n: "nextUnresolved",
    v: "toggleViewed",
    c: "comment",
    "?": "help",
  };
  return map[event.key] ?? null;
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
