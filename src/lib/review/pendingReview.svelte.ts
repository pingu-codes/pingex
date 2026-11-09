// The reviewer's pending review of one PR, as the review page edits it:
// typed entries (line or range comments, suggestions, replies held for the
// review, accepted Findings), a summary and a verdict. Every change is
// saved to the profile database through the desk, so the review survives
// reloads and restarts; Submit posts it as one GitHub review.
//
// Agent reviews add a Finding with
//   pending.add({ kind: "acceptedFinding", findingId, anchor, body })
// and ask `pending.hasFinding(findingId)` to show it as added.

import { commands } from "$lib/services/homeRouting";
import type { PendingEntry, PendingReview, ReviewVerdict } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

type DistributiveOmit<T, K extends PropertyKey> = T extends unknown ? Omit<T, K> : never;

/** An entry before the store gives it an id. */
export type NewPendingEntry = DistributiveOmit<PendingEntry, "id">;

/** What an entry's text can change to. */
export type EntryEdit = { body?: string; replacement?: string };

let nextId = 0;

export class PendingReviewStore {
  entries = $state<PendingEntry[]>([]);
  summary = $state("");
  verdict = $state<ReviewVerdict>("comment");
  /** The head the entries are written against. */
  headSha = $state("");
  /** The head a restored review was written at, when the PR has moved on
   *  since: its line anchors may be off. */
  writtenAt = $state<string | null>(null);
  loaded = $state(false);
  /** The stored review couldn't be read. Saves and Submit hold off until
   *  `retryLoad` succeeds, so what is stored isn't overwritten unseen. */
  loadError = $state<string | null>(null);
  /** The last save failed: what's on screen isn't stored yet. */
  saveError = $state<string | null>(null);
  /** Called when the reviewer takes an accepted Finding back out of the
   *  review (removed, or the review discarded), not when it is submitted. */
  onRemoveFinding: ((findingId: string) => void) | null = null;

  #saving: Promise<unknown> = Promise.resolve();
  /** The PR's head as the page last showed it. */
  #latestHead = "";

  constructor(
    readonly repoDir: string,
    readonly number: number,
  ) {}

  /** Restore the stored review, if any, for a PR now at `headSha`. */
  async load(headSha: string): Promise<void> {
    this.headSha = headSha;
    this.#latestHead = headSha;
    await this.#readStored();
    this.loaded = true;
  }

  /** Read the stored review again after `loadError`. What was written in the
   *  meantime stays, after what was stored. */
  async retryLoad(): Promise<void> {
    await this.#readStored();
    if (!this.loadError && !this.#isBlank) this.#autosave();
  }

  /** The PR's head as the page now shows it. A review with nothing in it
   *  follows; one with content stays on the commit it was written against. */
  syncHead(head: string): void {
    this.#latestHead = head;
    if (!this.loaded || !this.#isBlank) return;
    this.headSha = head;
    this.writtenAt = null;
  }

  get #isBlank(): boolean {
    return this.entries.length === 0 && !this.summary.trim();
  }

  async #readStored(): Promise<void> {
    let stored: PendingReview | null;
    try {
      stored = await commands.reviewPendingReview(this.repoDir, this.number);
      this.loadError = null;
    } catch (cause) {
      this.loadError = errorMessage(cause);
      return;
    }
    if (!stored) return;
    this.entries = [...stored.entries, ...this.entries];
    this.summary ||= stored.summary;
    this.verdict = stored.verdict;
    if (stored.headSha && stored.headSha !== this.#latestHead) this.writtenAt = stored.headSha;
    this.headSha = stored.headSha || this.#latestHead;
  }

  add(entry: NewPendingEntry): string {
    const id = `${entry.kind}-${Date.now().toString(36)}-${nextId++}`;
    this.entries = [...this.entries, { ...entry, id } as PendingEntry];
    this.#autosave();
    return id;
  }

  update(id: string, edit: EntryEdit): void {
    this.entries = this.entries.map((entry) => {
      if (entry.id !== id) return entry;
      const next = { ...entry, body: edit.body ?? entry.body };
      if (next.kind === "suggestion" && edit.replacement !== undefined) next.replacement = edit.replacement;
      return next;
    });
    this.#autosave();
  }

  remove(id: string): void {
    const removed = this.entries.find((entry) => entry.id === id);
    this.entries = this.entries.filter((entry) => entry.id !== id);
    this.#autosave();
    if (removed?.kind === "acceptedFinding") this.onRemoveFinding?.(removed.findingId);
  }

  /** Change the summary; call `save` when the reviewer leaves the box. */
  setSummary(text: string): void {
    this.summary = text;
  }

  setVerdict(verdict: ReviewVerdict): void {
    this.verdict = verdict;
    this.#autosave();
  }

  hasFinding(findingId: string): boolean {
    return this.entries.some((entry) => entry.kind === "acceptedFinding" && entry.findingId === findingId);
  }

  /** Save without a caller waiting: a failure shows through `saveError`. */
  #autosave(): void {
    this.save().catch(() => {});
  }

  /** Write the review as it is now. Saves run one after another. */
  save(): Promise<unknown> {
    if (this.loadError) return Promise.resolve();
    const review: PendingReview = $state.snapshot({
      headSha: this.headSha,
      entries: this.entries,
      summary: this.summary,
      verdict: this.verdict,
      updatedAt: 0,
    }) as PendingReview;
    this.#saving = this.#saving
      .catch(() => {})
      .then(() => commands.reviewSavePendingReview(this.repoDir, this.number, review))
      .then(
        () => {
          this.saveError = null;
        },
        (cause) => {
          this.saveError = errorMessage(cause);
          throw cause;
        },
      );
    return this.#saving;
  }

  /** Post the review as one GitHub review and start afresh. Throws, keeping
   *  every entry, when GitHub refuses it. */
  async submit(): Promise<void> {
    if (this.loadError) throw new Error(this.loadError);
    await this.save();
    await commands.reviewSubmitPendingReview(this.repoDir, this.number);
    this.clear();
  }

  async discard(): Promise<void> {
    const findings = this.entries.flatMap((entry) => (entry.kind === "acceptedFinding" ? [entry.findingId] : []));
    this.clear();
    for (const findingId of findings) this.onRemoveFinding?.(findingId);
    await this.#saving.catch(() => {});
    await commands.reviewDiscardPendingReview(this.repoDir, this.number);
  }

  private clear() {
    this.entries = [];
    this.summary = "";
    this.verdict = "comment";
    this.writtenAt = null;
    this.headSha = this.#latestHead;
  }
}
