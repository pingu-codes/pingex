/**
 * Git mutations fail with a `<kind>: <message>` string (see
 * `src-tauri/src/git/run.rs`). The kind picks a hint; the message is shown.
 */
export type GitErrorKind =
  | "auth"
  | "nonFastForward"
  | "noUpstream"
  | "conflict"
  | "dirtyTree"
  | "hookRejected"
  | "other";

export interface GitError {
  kind: GitErrorKind;
  message: string;
  /** Extra lines after the message, e.g. what a rejecting hook printed. */
  detail: string | null;
}

const KINDS: GitErrorKind[] = [
  "auth",
  "nonFastForward",
  "noUpstream",
  "conflict",
  "dirtyTree",
  "hookRejected",
  "other",
];

export function parseGitError(cause: unknown): GitError {
  const raw = cause instanceof Error ? cause.message : String(cause ?? "");
  const [first, ...rest] = raw.split("\n");
  const match = /^([a-zA-Z]+): (.*)$/.exec(first);
  const kind = match && (KINDS as string[]).includes(match[1]) ? (match[1] as GitErrorKind) : "other";
  const message = match && kind !== "other" ? match[2] : match && match[1] === "other" ? match[2] : first;
  const detail = rest.join("\n").trim();
  return { kind, message: message.trim() || "Git operation failed", detail: detail.length > 0 ? detail : null };
}

/** A one-line next step for the error kind, or null when the message suffices. */
export function hintFor(kind: GitErrorKind): string | null {
  switch (kind) {
    case "auth":
      return "Run a git command in a terminal to sign in, then try again.";
    case "nonFastForward":
      return "Pull first, then push again.";
    case "noUpstream":
      return "Publish the branch to create an upstream.";
    case "conflict":
      return "Resolve the conflicts in a terminal, then refresh.";
    case "dirtyTree":
      return "Commit or discard the changes, or switch anyway.";
    case "hookRejected":
      return "Fix what the hook reported and commit again.";
    default:
      return null;
  }
}
