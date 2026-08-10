import type { HandoffOpen } from "$lib/types";

/**
 * Pure helpers shared by the thread-header Handoff menu and the incoming
 * `codex://` deep-link flow. Kept free of Svelte/Tauri so they can be unit
 * tested directly.
 */

/** Short, human display name for a Codex home (the final path segment). */
export function shortHomeName(home: string | null | undefined): string {
  if (!home) return "home";
  const trimmed = home.replace(/\/+$/, "");
  const segment = trimmed.split("/").pop();
  return segment && segment.length > 0 ? segment : trimmed;
}

/** Final path segment of a directory, for the breadcrumb branch/worktree slot. */
export function dirName(path: string | null | undefined): string {
  if (!path) return "";
  const trimmed = path.replace(/\/+$/, "");
  return trimmed.split("/").pop() ?? trimmed;
}

/**
 * A thread's `cwd` "belongs to" the requested path when they are equal or the
 * cwd is nested under the requested directory. Used to warn when a deep link's
 * requested worktree does not match the thread that actually opened.
 */
export function cwdBelongsTo(threadCwd: string | null | undefined, requestedPath: string | null | undefined): boolean {
  if (!requestedPath) return true;
  if (!threadCwd) return false;
  const normalize = (value: string) => value.replace(/\/+$/, "");
  const cwd = normalize(threadCwd);
  const requested = normalize(requestedPath);
  return cwd === requested || cwd.startsWith(`${requested}/`);
}

/** Whether the Handoff menu can act: needs a live thread, a cwd, and a home. */
export function canHandoff(
  threadId: string | null,
  cwd: string | null | undefined,
  home: string | null | undefined,
): boolean {
  return Boolean(threadId && cwd && home);
}

/** A short, actionable message for an incoming handoff we cannot fulfil as-is. */
export function handoffHomeIssue(open: HandoffOpen): string | null {
  if (open.homeMatches) return null;
  if (!open.homeExists) {
    return open.requestedHome
      ? `The Codex home this link needs (${open.requestedHome}) was not found.`
      : "The Codex home this link needs was not found.";
  }
  return null;
}
