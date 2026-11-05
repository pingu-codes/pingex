// One PR's agent Findings as the review page shows them: inline drafts
// only the reviewer sees until added to the pending review. The desk stores
// each Finding's state (draft, added, dismissed with a reason, merged into a
// human comment); the pending review holds the text an added one posts.

import { agentReviews } from "$lib/review/agentReviews.svelte";
import type { PendingReviewStore } from "$lib/review/pendingReview.svelte";
import type { DiffSide } from "$lib/review/reviewPage";
import { commands } from "$lib/services/homeRouting";
import type { AgentReview, DiffAnchor, DismissReason, Finding, PendingEntry } from "$lib/types";

/** The text an added Finding posts, before the reviewer edits it: an
 *  ordinary comment, with no agent marker. */
export function findingBody(finding: Finding): string {
  const title = finding.title.trim();
  const body = finding.body.trim();
  if (!title) return body;
  if (!body) return title;
  return `**${title}**\n\n${body}`;
}

/** Where a Finding sits in the diff, as a pending entry anchors it. */
export function findingAnchor(finding: Finding): DiffAnchor {
  const end = finding.endLine ?? finding.line;
  return {
    path: finding.path,
    side: finding.side,
    line: end,
    startLine: end > finding.line ? finding.line : null,
  };
}

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
  get shown(): Finding[] {
    return this.findings.filter((finding) => finding.state !== "dismissed");
  }

  /** The Findings to show under one diff line: anchored at its last line. */
  at(path: string, side: DiffSide, line: number): Finding[] {
    return this.shown.filter(
      (finding) => finding.path === path && finding.side === side && (finding.endLine ?? finding.line) === line,
    );
  }

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
