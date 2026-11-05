// How an agent Finding becomes a pending-review comment: its text and its
// anchor. Pure, so the queue and the review page share it.

import type { DiffAnchor, Finding, PendingEntry } from "$lib/types";

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

/** The pending-review entry for a Finding added as the agent wrote it. */
export function acceptedEntry(finding: Finding, id: string): PendingEntry {
  return {
    kind: "acceptedFinding",
    id,
    findingId: finding.id,
    anchor: findingAnchor(finding),
    body: findingBody(finding),
  };
}
