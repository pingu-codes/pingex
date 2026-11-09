// One PR's agent Findings as the review page shows them: inline drafts
// only the reviewer sees until added to the pending review. The desk stores
// each Finding's state (draft, added, dismissed with a reason, merged into a
// human comment); the pending review holds the text an added one posts.

import { agentReviews } from "$lib/review/agentReviews.svelte";
import { findingAnchor, findingBody } from "$lib/review/findingText";
import type { PendingReviewStore } from "$lib/review/pendingReview.svelte";
import { type DiffSide, groupBy } from "$lib/review/reviewPage";
import { commands } from "$lib/services/homeRouting";
import type { AgentReview, DismissReason, Finding, PendingEntry } from "$lib/types";

export { findingAnchor, findingBody };

const NONE: Finding[] = [];
const lineKey = (path: string, side: string, line: number) => `${path}:${side}:${line}`;
/** A Finding sits under its last line. */
const anchorLine = (finding: Finding) => finding.endLine ?? finding.line;

export const DISMISS_LABELS: Record<DismissReason, string> = {
  notAProblem: "Not a problem",
  alreadyRaised: "Already raised in a comment",
  outOfScope: "Out of scope for this PR",
  wrong: "Wrong",
};

export class FindingsStore {
  findings = $state<Finding[]>([]);
  loaded = $state(false);
  error = $state<string | null>(null);

  constructor(
    readonly repoDir: string,
    readonly number: number,
    readonly pending: PendingReviewStore,
  ) {
    // Taking an added Finding out of the review makes it a draft again.
    pending.onRemoveFinding = (findingId) => {
      const finding = this.findings.find((entry) => entry.id === findingId);
      if (finding?.state === "added") void this.setState(finding, "draft");
    };
  }

  /** The PR's agent review runs, newest first. */
  get runs(): AgentReview[] {
    return (agentReviews.byRepo[this.repoDir] ?? []).filter((review) => review.number === this.number);
  }

  get latest(): AgentReview | null {
    return this.runs[0] ?? null;
  }

  async load(): Promise<void> {
    try {
      this.findings = await commands.reviewFindings(this.repoDir, this.number);
      this.error = null;
    } catch (cause) {
      this.error = String(cause);
    } finally {
      this.loaded = true;
    }
  }

  /** Findings still in play: drafts, added and merged, not dismissed. */
  readonly shown = $derived(this.findings.filter((finding) => finding.state !== "dismissed"));

  /** `shown` by the line each one sits under, `path:side:line`: every diff
   *  row asks `at`, so it is a lookup, not a scan. */
  readonly #byLine = $derived(
    groupBy(this.shown, (finding) => lineKey(finding.path, finding.side, anchorLine(finding))),
  );

  /** The Findings to show under one diff line: anchored at its last line. */
  at(path: string, side: DiffSide, line: number): Finding[] {
    return this.#byLine.get(lineKey(path, side, line)) ?? NONE;
  }

  /** Per path, the lines (`side:line`) a shown Finding sits under. */
  readonly linesByPath = $derived.by(() => {
    const lines = new Map<string, Set<string>>();
    for (const finding of this.shown) {
      const key = `${finding.side}:${anchorLine(finding)}`;
      const own = lines.get(finding.path);
      if (own) own.add(key);
      else lines.set(finding.path, new Set([key]));
    }
    return lines;
  });

  /** Open Findings per path, for the file tree and the Agent findings
   *  filter: drafts and added ones, not merged or dismissed. */
  get counts(): Record<string, number> {
    const counts: Record<string, number> = {};
    for (const finding of this.findings) {
      if (finding.state === "draft" || finding.state === "added") {
        counts[finding.path] = (counts[finding.path] ?? 0) + 1;
      }
    }
    return counts;
  }

  /** How many of the runs' Findings were added and dismissed. */
  get tally(): { total: number; added: number; dismissed: number } {
    return {
      total: this.findings.length,
      added: this.findings.filter((finding) => this.isAdded(finding)).length,
      dismissed: this.findings.filter((finding) => finding.state === "dismissed").length,
    };
  }

  /** The pending review's entry for an added Finding. */
  entryFor(finding: Finding): Extract<PendingEntry, { kind: "acceptedFinding" }> | null {
    return (
      this.pending.entries.find(
        (entry): entry is Extract<PendingEntry, { kind: "acceptedFinding" }> =>
          entry.kind === "acceptedFinding" && entry.findingId === finding.id,
      ) ?? null
    );
  }

  isAdded(finding: Finding): boolean {
    return this.pending.hasFinding(finding.id) || finding.state === "added";
  }

  /** Add to my review, with the Finding's text or the reviewer's edit. */
  async add(finding: Finding, body = findingBody(finding)): Promise<void> {
    const entry = this.entryFor(finding);
    if (entry) this.pending.update(entry.id, { body });
    else this.pending.add({ kind: "acceptedFinding", findingId: finding.id, anchor: findingAnchor(finding), body });
    await this.setState(finding, "added");
  }

  /** Put Findings the desk already marked added (a run with "Keep findings
   *  as drafts" off) into the pending review, once. */
  adoptAdded(): void {
    for (const finding of this.findings) {
      if (finding.state === "added" && !this.pending.hasFinding(finding.id)) {
        this.pending.add({
          kind: "acceptedFinding",
          findingId: finding.id,
          anchor: findingAnchor(finding),
          body: findingBody(finding),
        });
      }
    }
  }

  /** Take it back out of the review. */
  removeFromReview(finding: Finding): void {
    const entry = this.entryFor(finding);
    if (entry) this.pending.remove(entry.id);
    else void this.setState(finding, "draft");
  }

  async dismiss(finding: Finding, reason: DismissReason, note: string | null = null): Promise<void> {
    const entry = this.entryFor(finding);
    if (entry) {
      // Removing would put it back to draft; dismissing wins.
      this.pending.entries = this.pending.entries.filter((other) => other.id !== entry.id);
      void this.pending.save();
    }
    await this.setState(finding, "dismissed", reason, note);
  }

  async restore(finding: Finding): Promise<void> {
    await this.setState(finding, "draft");
  }

  private async setState(
    finding: Finding,
    state: Finding["state"],
    reason: DismissReason | null = null,
    note: string | null = null,
  ): Promise<void> {
    const updated = await commands.reviewSetFindingState(this.repoDir, finding.id, state, reason, note);
    this.findings = this.findings.map((entry) => (entry.id === finding.id ? updated : entry));
  }
}
