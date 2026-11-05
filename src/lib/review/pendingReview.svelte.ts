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

  #saving: Promise<unknown> = Promise.resolve();

  constructor(
    readonly repoDir: string,
    readonly number: number,
  ) {}

  /** How many comments Submit posts, replies and accepted Findings included. */
  get commentCount(): number {
    return this.entries.length;
  }

  /** Restore the stored review, if any, for a PR now at `headSha`. */
  async load(headSha: string): Promise<void> {
    this.headSha = headSha;
    const stored = await commands.reviewPendingReview(this.repoDir, this.number).catch(() => null);
    if (stored) {
      this.entries = stored.entries;
      this.summary = stored.summary;
      this.verdict = stored.verdict;
      if (stored.headSha && stored.headSha !== headSha) this.writtenAt = stored.headSha;
      this.headSha = stored.headSha || headSha;
    }
    this.loaded = true;
  }

  add(entry: NewPendingEntry): string {
    const id = `${entry.kind}-${Date.now().toString(36)}-${nextId++}`;
    this.entries = [...this.entries, { ...entry, id } as PendingEntry];
    void this.save();
    return id;
  }

  update(id: string, edit: EntryEdit): void {
    this.entries = this.entries.map((entry) => {
      if (entry.id !== id) return entry;
      const next = { ...entry, body: edit.body ?? entry.body };
      if (next.kind === "suggestion" && edit.replacement !== undefined) next.replacement = edit.replacement;
      return next;
    });
    void this.save();
  }

  remove(id: string): void {
    this.entries = this.entries.filter((entry) => entry.id !== id);
    void this.save();
  }

  /** Change the summary; call `save` when the reviewer leaves the box. */
  setSummary(text: string): void {
    this.summary = text;
  }

  setVerdict(verdict: ReviewVerdict): void {
    this.verdict = verdict;
    void this.save();
  }

  hasFinding(findingId: string): boolean {
    return this.entries.some((entry) => entry.kind === "acceptedFinding" && entry.findingId === findingId);
  }

  /** Write the review as it is now. Saves run one after another. */
  save(): Promise<unknown> {
    const review: PendingReview = $state.snapshot({
      headSha: this.headSha,
      entries: this.entries,
      summary: this.summary,
      verdict: this.verdict,
      updatedAt: 0,
    }) as PendingReview;
    this.#saving = this.#saving
      .catch(() => {})
      .then(() => commands.reviewSavePendingReview(this.repoDir, this.number, review));
    return this.#saving;
  }

  /** Post the review as one GitHub review and start afresh. Throws, keeping
   *  every entry, when GitHub refuses it. */
  async submit(): Promise<void> {
    await this.save();
    await commands.reviewSubmitPendingReview(this.repoDir, this.number);
    this.clear();
  }

  async discard(): Promise<void> {
    this.clear();
    await this.#saving.catch(() => {});
    await commands.reviewDiscardPendingReview(this.repoDir, this.number);
  }

  private clear() {
    this.entries = [];
    this.summary = "";
    this.verdict = "comment";
    this.writtenAt = null;
  }
}
