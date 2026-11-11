// One pull request open for review, from opening it to submitting the
// review: the PR under review. It owns the order the reads and writes must
// run in, so the page that shows it only lays it out.
//
// - It follows the PR's head. Each new head re-reads the viewed marks (a
//   changed file is unread again), keeps an empty Pending review on that
//   head, and re-reads "changes since" when they are showing. An answer for
//   a head the PR has since left is dropped.
// - The stored Pending review is read once, beside the first head's marks.
// - Reads by SHA ("changes since", a whole file) and marking the Seen head
//   fetch the PR's refs first (ADR 0004).
// - Submit posts the review, then marks the head as seen, then re-reads.
// - Every write through `write` re-reads the PR after it.
//
// Construct it during component setup: it reads through `query`, whose
// effects belong to the component.

import { untrack } from "svelte";
import type { PendingReviewStore } from "$lib/review/pendingReview.svelte";
import { ReviewFiles } from "$lib/review/reviewFiles.svelte";
import { fileBlob, prPlace } from "$lib/review/reviewPage";
import { commands } from "$lib/services/homeRouting";
import { type QueryHandle, query } from "$lib/services/queryCache.svelte";
import type { ChangesSince, PrDetail, PrFile, PrFileContents } from "$lib/types";
import { submitState } from "$lib/utils/actionState.svelte";
import { errorMessage } from "$lib/utils/errors";

/** The viewed marks and folds as a caller may change them: marking viewed
 *  goes through `PrUnderReview.setViewed`, which saves the mark. */
export type ReviewFolds = Omit<ReviewFiles, "applyMarks" | "setViewed">;

export class PrUnderReview {
  readonly repoDir: string;
  readonly number: number;
  readonly files: ReviewFolds;
  /** The PR opened on "changes since", as it was left or because the inbox
   *  asked, so there is no scroll position to go back to. */
  readonly opensOnSince: boolean;

  readonly #pending: PendingReviewStore;
  readonly #viewer: () => string;
  readonly #files: ReviewFiles;
  readonly #detail: QueryHandle<PrDetail>;
  readonly #action = submitState();
  readonly #left: ReturnType<typeof prPlace>;
  /** The head the viewed marks were last read at. */
  #readHead: string | null = null;
  #pendingLoad: Promise<void> | null = null;
  #ready = $state(false);
  #changes = $state<"all" | "since">("all");
  #since = $state<ChangesSince | null>(null);
  /** "Changes since" was asked for and not taken back. */
  #wantSince = false;
  /** The "changes since" read in flight: one at a time, and it follows the head. */
  #sinceRead = $state<Promise<void> | null>(null);

  constructor(options: {
    repoDir: string;
    number: number;
    /** Edited by the page and by agent Findings; this loads it and keeps it on the right head. */
    pending: PendingReviewStore;
    /** Who is reviewing. GitHub takes only a comment from a PR's author. */
    viewer: () => string;
  }) {
    this.repoDir = options.repoDir;
    this.number = options.number;
    this.#pending = options.pending;
    this.#viewer = options.viewer;
    this.#left = prPlace(this.repoDir, this.number);
    this.opensOnSince = this.#left.changes === "since";
    this.#files = new ReviewFiles(this.#left.collapsed);
    this.files = this.#files;
    // Cached: a remount shows the PR at once and refreshes it behind. A write
    // through `commands` marks it stale, which re-reads it.
    this.#detail = query("reviewPrDetail", () => [this.repoDir, this.number]);

    // Each time the detail lands: the first read, a refresh, after a write.
    $effect(() => {
      const next = this.detail;
      if (next) untrack(() => this.#follow(next));
    });
    $effect(() => {
      if (this.#files.collapsedKnown) this.#left.collapsed = [...this.#files.collapsed];
    });
  }

  /** The first head's Pending review and viewed marks are in. */
  get ready(): boolean {
    return this.#ready;
  }

  /** Every changed file, or only the changes since the Seen head. */
  get changes(): "all" | "since" {
    return this.#changes;
  }

  /** What "changes since" shows, once read. */
  get since(): ChangesSince | null {
    return this.#since;
  }

  get detail(): PrDetail | null {
    return this.#detail.value ?? null;
  }

  /** Why the PR couldn't be read, while there is nothing to show. */
  get loadError(): string | null {
    return this.detail ? null : this.#detail.error;
  }

  /** The files on show: every changed file, or the changes since the Seen head. */
  get shownFiles(): PrFile[] {
    return this.#changes === "since" && this.#since ? this.#since.files : (this.detail?.files ?? []);
  }

  /** GitHub refuses Approve and Request changes from a PR's author. */
  get ownPr(): boolean {
    const viewer = this.#viewer();
    return !!this.detail && !!viewer && this.detail.summary.author === viewer;
  }

  /** The Pending review is written against a head the PR has moved on from. */
  get stale(): boolean {
    const head = this.#pending.headSha;
    return !!this.detail && !!head && head !== this.detail.headSha;
  }

  /** A write, submit or "changes since" read is running. */
  get busy(): boolean {
    return this.#action.busy || this.#sinceRead !== null;
  }

  /** Why the last of those failed, or `null`. */
  get error(): string | null {
    return this.#action.error;
  }

  /** Show every changed file, or read and show the changes since the Seen
   *  head. Those follow the head until "all" is asked for. */
  showChanges(value: "all" | "since"): Promise<void> {
    if (value === "all" || !this.detail) {
      this.#wantSince = false;
      this.#setChanges("all");
      return Promise.resolve();
    }
    this.#wantSince = true;
    this.#sinceRead ??= this.#readSince()
      .catch((cause) => {
        this.#action.error = errorMessage(cause);
      })
      .finally(() => {
        this.#sinceRead = null;
      });
    return this.#sinceRead;
  }

  /** Mark files viewed or not (which folds or unfolds them) and save the marks. */
  setViewed(targets: PrFile[], value: boolean): void {
    this.#files.setViewed(targets, value);
    if (targets.length === 0) return;
    commands.reviewSetViewed(this.repoDir, this.number, targets.map(fileBlob), value).catch((cause) => {
      this.#action.error = errorMessage(cause);
    });
  }

  /** Post the Pending review, mark the head as seen, and re-read the PR.
   *  False when it didn't post; `error` says why. */
  async submit(): Promise<boolean> {
    const at = this.detail;
    if (!at || this.#action.busy) return false;
    if (this.ownPr && this.#pending.verdict !== "comment") this.#pending.setVerdict("comment");
    const posted = await this.#action.run(() => this.#pending.submit());
    if (!posted) return false;
    // What "changes since your review" measures from. Best effort: a
    // PRs-only repository has no clone to pin it in.
    await this.#fetchRefs()
      .then(() => commands.reviewMarkSeen(this.repoDir, this.number, at.headSha))
      .catch(() => {});
    await this.#reload();
    return true;
  }

  /** Run a write to the PR, then re-read it. False when either failed;
   *  `error` says why. */
  write(work: () => Promise<unknown>): Promise<boolean> {
    return this.#action.run(async () => {
      await work();
      await this.#reload();
    });
  }

  /** A file at one of the PR's commits, read by SHA once the PR's refs are
   *  fetched. Call it during component setup, like `query`. */
  fileAt(at: () => readonly [sha: string, path: string] | null): QueryHandle<PrFileContents> {
    let fetched = $state(false);
    void this.#fetchRefs()
      .catch(() => {})
      .then(() => {
        fetched = true;
      });
    return query("reviewFile", () => {
      const target = fetched ? at() : null;
      return target ? [this.repoDir, target[0], target[1]] : null;
    });
  }

  #follow(next: PrDetail): void {
    this.#pending.syncHead(next.headSha);
    if (this.#readHead === next.headSha) return;
    const first = this.#readHead === null;
    this.#readHead = next.headSha;
    void this.#readReviewState(next);
    if (first ? this.opensOnSince : this.#changes === "since") void this.showChanges("since");
  }

  /** The stored Pending review (once) and the viewed marks at `next`'s head,
   *  read side by side. */
  async #readReviewState(next: PrDetail): Promise<void> {
    this.#pendingLoad ??= this.#pending.loaded ? Promise.resolve() : this.#pending.load(next.headSha);
    const [, marked] = await Promise.all([
      this.#pendingLoad,
      commands.reviewViewedFiles(this.repoDir, this.number, next.files.map(fileBlob)).catch(() => [] as string[]),
    ]);
    if (this.detail) this.#pending.syncHead(this.detail.headSha);
    if (this.#readHead !== next.headSha) return;
    this.#files.applyMarks(marked);
    this.#ready = true;
  }

  /** Until the answer is for the head the PR is at: a push mid-read reads
   *  again, with the refs fetched for the new head. */
  async #readSince(): Promise<void> {
    for (let at = this.detail; at && this.#wantSince; at = this.detail) {
      await this.#fetchRefs();
      const read = await commands.reviewChangesSince(this.repoDir, this.number, at.baseSha, at.headSha);
      if (!this.#wantSince) return;
      if (this.detail?.headSha !== at.headSha) continue;
      this.#since = read;
      this.#setChanges("since");
      return;
    }
  }

  #setChanges(value: "all" | "since"): void {
    this.#changes = value;
    this.#left.changes = value;
  }

  /** The PR's commits into its hidden refs, which reads by SHA need. */
  #fetchRefs(): Promise<unknown> {
    return commands.reviewFetchPrs(this.repoDir, [this.number]);
  }

  /** Re-read the PR. A write through `commands` already marked it stale, so
   *  this joins that read. */
  async #reload(): Promise<void> {
    await this.#detail.reload();
    if (this.#detail.error) this.#action.error = this.#detail.error;
  }
}
