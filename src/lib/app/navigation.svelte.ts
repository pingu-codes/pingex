/**
 * What the content area is showing. The views are mutually exclusive, so the
 * state is a single record that `setView` rewrites wholesale — opening one view
 * always clears the others rather than every call site remembering to reset six
 * fields. Projects are held by path and resolved against `appData`, so a
 * bootstrap refresh transparently updates them.
 */
import { projectByPath, projectForCwd, projects, threadById } from "$lib/app/appData.svelte";
import { touchThread } from "$lib/layout/sessionFocus.svelte";
import type { Project, ThreadSummary } from "$lib/types";

export type View = {
  /** Live thread being viewed. */
  threadId: string | null;
  /** Project context for the sidebar/header. */
  projectPath: string | null;
  /** Draft thread directory (a new, unsent thread). */
  draftCwd: string | null;
  /** Repository whose worktrees view is open. */
  worktreesPath: string | null;
  /** Repository whose pull-request review view is open. */
  reviewPath: string | null;
  /** Project whose detail view (instructions, sources, search) is open. */
  detailPath: string | null;
};

const EMPTY: View = {
  threadId: null,
  projectPath: null,
  draftCwd: null,
  worktreesPath: null,
  reviewPath: null,
  detailPath: null,
};

export const view = $state<View & { epoch: number }>({
  ...EMPTY,
  /** Bumped on every navigation so keyed views remount with fresh state. */
  epoch: 0,
});

/**
 * Show exactly the view described by `patch`; every field it omits is cleared.
 * `remount: false` keeps the epoch (and so the live ThreadView) intact — used
 * when a draft becomes a real thread mid-turn.
 */
export function setView(patch: Partial<View>, options: { remount?: boolean } = {}): void {
  Object.assign(view, EMPTY, patch);
  if (options.remount !== false) view.epoch += 1;
  // Opening a thread counts as interacting with it for the sidebar's
  // session-focus view.
  touchThread(patch.threadId);
}

export function currentProject(): Project | null {
  return projectByPath(view.projectPath);
}

export function worktreesRepo(): Project | null {
  return projectByPath(view.worktreesPath);
}

export function reviewRepo(): Project | null {
  return projectByPath(view.reviewPath);
}

export function detailProject(): Project | null {
  return projectByPath(view.detailPath);
}

export function selectedThreadInfo(): ThreadSummary | null {
  return threadById(view.threadId);
}

export function goHome(): void {
  setView({});
}

/** Open a thread from the sidebar, where the project is already known. */
export function openThread(project: Project, threadId: string): void {
  setView({ projectPath: project.path, threadId });
}

/** Open a thread by id, resolving its project from the thread's cwd. */
export function openThreadById(threadId: string): void {
  const thread = threadById(threadId);
  if (!thread) return;
  setView({ threadId, projectPath: projectForCwd(thread.cwd)?.path ?? view.projectPath });
}

/** Open a thread found outside the sidebar (search hit, archived thread). */
export function openThreadInCwd(threadId: string, cwd: string): void {
  setView({ threadId, projectPath: projectForCwd(cwd)?.path ?? null });
}

/**
 * Adopt a thread into the current view without remounting it: a draft that has
 * just been created is already running its first turn.
 */
export function adoptThread(threadId: string): void {
  view.threadId = threadId;
  view.draftCwd = null;
}

export function openParentThread(): void {
  const parent = selectedThreadInfo()?.parentThreadId;
  if (parent) openThreadById(parent);
}

/** Start a draft thread in a project (defaults to the current one). */
export function newThread(inProject?: Project): void {
  const project = inProject ?? currentProject() ?? projects().find((candidate) => !candidate.archived) ?? null;
  if (!project) return;
  setView({ projectPath: project.path, draftCwd: project.path });
}

/** Start a draft thread whose cwd is an exact directory (e.g. a worktree). */
export function newThreadInDir(cwd: string): void {
  setView({ draftCwd: cwd, projectPath: projectForCwd(cwd)?.path ?? view.projectPath });
}

export function openProjectDetail(project: Project): void {
  setView({ projectPath: project.path, detailPath: project.path });
}

export function openWorktrees(project: Project): void {
  setView({ projectPath: project.path, worktreesPath: project.path });
}

export function openReview(project: Project): void {
  setView({ projectPath: project.path, reviewPath: project.path });
}

/** "Open in app" from the Worktrees view: focus the matching project. */
export function focusProjectPath(path: string): void {
  setView({ projectPath: projectForCwd(path)?.path ?? view.projectPath });
}

/** Clear the selection when the thread it points at goes away. */
export function clearThread(threadId: string): void {
  if (view.threadId !== threadId) return;
  view.threadId = null;
  view.epoch += 1;
}
