// Notes left on the Changes page, and the one message they become for the
// agent: grouped by repository and segment, each anchored to `path:lines`
// with the lines it is about quoted.

import type { DiffSide } from "$lib/review/reviewPage";
import type { PendingEntry } from "$lib/types";

export type ChangeNote = {
  id: string;
  /** The repository's top level. */
  root: string;
  repoLabel: string;
  segmentId: string;
  /** "branch `feat/x` (#12)", "uncommitted changes". */
  segmentLabel: string;
  path: string;
  side: DiffSide;
  /** The first line of a range; `null` for one line. */
  startLine: number | null;
  line: number;
  /** The selected lines as they read in the diff. */
  excerpt: string;
  body: string;
  /** A suggested replacement for the selected lines. */
  replacement: string | null;
};

const EXCERPT_LINES = 6;

/** The note as FileDiff draws a pending entry under its line. */
export function noteEntry(note: ChangeNote): PendingEntry {
  const anchor = { path: note.path, side: note.side, line: note.line, startLine: note.startLine };
  return note.replacement === null
    ? { kind: "comment", id: note.id, anchor, body: note.body }
    : { kind: "suggestion", id: note.id, anchor, body: note.body, replacement: note.replacement };
}

function lines(note: ChangeNote): string {
  return note.startLine !== null && note.startLine < note.line ? `${note.startLine}-${note.line}` : `${note.line}`;
}

function quote(excerpt: string): string {
  const all = excerpt.split("\n");
  const shown = all.slice(0, EXCERPT_LINES).map((line) => `  > ${line}`);
  if (all.length > EXCERPT_LINES) shown.push(`  > … ${all.length - EXCERPT_LINES} more lines`);
  return shown.join("\n");
}

function fence(text: string): string {
  // A fence longer than any backtick run inside, so the block cannot close early.
  const longest = Math.max(2, ...(text.match(/`+/g) ?? []).map((run) => run.length));
  return "`".repeat(longest + 1);
}

/**
 * One message carrying every note, in the order they were left within each
 * repository and segment. Paths are relative to the repository named above
 * them, which is stated once with its folder.
 */
export function notesMessage(notes: readonly ChangeNote[]): string {
  if (notes.length === 0) return "";
  const groups = new Map<string, ChangeNote[]>();
  for (const note of notes) {
    const key = `${note.root}\u0000${note.segmentId}`;
    groups.set(key, [...(groups.get(key) ?? []), note]);
  }
  const intro = notes.length === 1 ? "I left a note on your changes:" : `I left ${notes.length} notes on your changes:`;
  const sections = [...groups.values()].map((group) => {
    const first = group[0];
    const heading = `**${first.repoLabel}** (\`${first.root}\`), ${first.segmentLabel}`;
    const items = group.map((note) => {
      const where = `- \`${note.path}:${lines(note)}\`${note.side === "LEFT" ? " (removed lines)" : ""}`;
      const parts = [where];
      if (note.excerpt.trim()) parts.push(quote(note.excerpt));
      parts.push(
        note.body
          .trim()
          .split("\n")
          .map((line) => `  ${line}`)
          .join("\n"),
      );
      if (note.replacement !== null) {
        const marks = fence(note.replacement);
        parts.push(`  Suggested replacement:\n  ${marks}\n${note.replacement.replace(/^/gm, "  ")}\n  ${marks}`);
      }
      return parts.join("\n");
    });
    return [heading, ...items].join("\n\n");
  });
  return [intro, ...sections].join("\n\n");
}
