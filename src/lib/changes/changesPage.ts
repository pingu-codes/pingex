// Pure helpers for the Changes page: which segment to open, the stub files
// the file rail reads, viewed marks that follow a file's contents, and when a
// running agent's changes are read again.

import type { ChangeSegment, ChangeSegmentFile, PrFile, RepoChanges, ThreadChanges } from "$lib/types";

/** A segment and the repository it belongs to. */
export type SegmentRef = { repo: RepoChanges; segment: ChangeSegment };

/** `root` and `segment.id` together name a segment across reads. */
export function segmentKey(root: string, segmentId: string): string {
  return `${root}\u0000${segmentId}`;
}

export function allSegments(changes: ThreadChanges | null | undefined): SegmentRef[] {
  return (changes?.repos ?? []).flatMap((repo) => repo.segments.map((segment) => ({ repo, segment })));
}

/**
 * The segment to show. The one asked for (by key) while it exists; else one
 * holding `path`; else the checked-out branch's working tree, then the
 * checked-out branch, then whatever comes first.
 */
export function pickSegment(
  changes: ThreadChanges | null | undefined,
  wanted: string | null,
  path: string | null = null,
): SegmentRef | null {
  const segments = allSegments(changes);
  if (segments.length === 0) return null;
  if (wanted) {
    const kept = segments.find((ref) => segmentKey(ref.repo.root, ref.segment.id) === wanted);
    if (kept) return kept;
    // A bare id (`/diff` asks for "uncommitted") matches in the first repository that has it.
    const byId = segments.find((ref) => ref.segment.id === wanted);
    if (byId) return byId;
  }
  if (path) {
    const holding = segments.find((ref) => ref.segment.files.some((file) => file.path === path));
    if (holding) return holding;
  }
  return (
    segments.find((ref) => ref.segment.kind === "uncommitted") ??
    segments.find((ref) => ref.segment.current) ??
    segments[0]
  );
}

/** What the file rail reads (path, status, counts) without any patch. */
export function stubFile(file: ChangeSegmentFile): PrFile {
  return {
    path: file.path,
    oldPath: file.oldPath,
    status: railStatus(file.status),
    additions: file.additions,
    deletions: file.deletions,
    patch: "",
    hunks: [],
    patchTruncated: false,
    blobSha: null,
  };
}

/** The listing's status words in the diff's vocabulary (GitHub's). */
export function railStatus(status: string): string {
  if (status === "untracked") return "added";
  if (status === "deleted") return "removed";
  return status;
}

/**
 * The segment's files in listing order, each with its patch when the diff
 * has one. A file the diff left out (past its cap) keeps an empty, truncated
 * patch, so it still shows and says why it has no lines.
 */
export function segmentFiles(segment: ChangeSegment, diff: PrFile[] | null | undefined): PrFile[] {
  const byPath = new Map((diff ?? []).map((file) => [file.path, file]));
  return segment.files.map((file) => {
    const patched = byPath.get(file.path);
    if (patched) return { ...patched, status: railStatus(file.status), oldPath: patched.oldPath ?? file.oldPath };
    return { ...stubFile(file), patchTruncated: diff != null };
  });
}

/** A viewed mark: the file at these contents in this segment. A new edit
 *  gives the file a new key, so it reads as unviewed again. */
export function viewedKey(root: string, segment: ChangeSegment, file: ChangeSegmentFile): string {
  return `${segmentKey(root, segment.id)}\u0000${file.path}\u0000${file.contentKey ?? `${file.additions}/${file.deletions}`}`;
}

/** Total files and lines across every repository's segments, for a summary. */
export function totals(changes: ThreadChanges | null | undefined): {
  files: number;
  additions: number;
  deletions: number;
} {
  const seen = new Set<string>();
  let additions = 0;
  let deletions = 0;
  for (const { repo, segment } of allSegments(changes)) {
    additions += segment.additions;
    deletions += segment.deletions;
    for (const file of segment.files) seen.add(`${repo.root}\u0000${file.path}`);
  }
  return { files: seen.size, additions, deletions };
}

/**
 * Coalesces "the agent changed something" into reads: each poke waits for a
 * quiet `delay`, but never longer than `maxWait` since the first unanswered
 * one, so a turn that edits non-stop still refreshes every few seconds.
 */
export class RefreshScheduler {
  #timer: ReturnType<typeof setTimeout> | null = null;
  #firstPoke: number | null = null;

  constructor(
    private readonly run: () => void,
    private readonly delay = 750,
    private readonly maxWait = 3000,
    private readonly now: () => number = Date.now,
  ) {}

  poke(): void {
    const at = this.now();
    this.#firstPoke ??= at;
    const wait = Math.max(0, Math.min(this.delay, this.#firstPoke + this.maxWait - at));
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = setTimeout(() => this.flush(), wait);
  }

  flush(): void {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
    this.#firstPoke = null;
    this.run();
  }

  dispose(): void {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = null;
    this.#firstPoke = null;
  }
}
