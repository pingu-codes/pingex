/**
 * Git mutations fail with a `<kind>: <message>` string (see
 * `src-tauri/src/git/run.rs`). The kind picks a hint; the message is shown.
 */

import { errorMessage } from "$lib/utils/errors";

const KINDS = ["auth", "nonFastForward", "noUpstream", "conflict", "dirtyTree", "hookRejected", "other"] as const;

export type GitErrorKind = (typeof KINDS)[number];

export interface GitError {
  kind: GitErrorKind;
  message: string;
  /** Extra lines after the message, e.g. what a rejecting hook printed. */
  detail: string | null;
}

export function parseGitError(cause: unknown): GitError {
  const raw = errorMessage(cause ?? "");
  const [first, ...rest] = raw.split("\n");
  const match = /^([a-zA-Z]+): (.*)$/.exec(first);
  const kind = match && (KINDS as readonly string[]).includes(match[1]) ? (match[1] as GitErrorKind) : "other";
  const message = match && kind !== "other" ? match[2] : match && match[1] === "other" ? match[2] : first;
  const detail = rest.join("\n").trim();
  return { kind, message: message.trim() || "Git operation failed", detail: detail.length > 0 ? detail : null };
}

/** A one-line next step for each error kind, or null when the message suffices. */
export const gitErrorHints: Record<GitErrorKind, string | null> = {
  auth: "Run a git command in a terminal to sign in, then try again.",
  nonFastForward: "Pull first, then push again.",
  noUpstream: "Publish the branch to create an upstream.",
  conflict: "Resolve the conflicts in a terminal, then refresh.",
  dirtyTree: "Commit or discard the changes, or switch anyway.",
  hookRejected: "Fix what the hook reported and commit again.",
  other: null,
};
