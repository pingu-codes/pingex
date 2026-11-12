/**
 * What the content area is showing. The views are mutually exclusive, so the
 * state is a single record that `setView` rewrites wholesale — opening one view
 * always clears the others rather than every call site remembering to reset six
 * fields. Projects are held by path and resolved against `appData`, so a
 * bootstrap refresh transparently updates them.
 *
 * Back retraces a trail of the views left on the way to a PR or a Stack page.
 * What each page had open (filters, tab, scroll) is its own, in `places.ts`.
 */
import { appData, projectByPath, projectForCwd, projects, threadById } from "$lib/app/appData.svelte";
import { pageThreadStart } from "$lib/app/pageThread";
import { stackHolding } from "$lib/inbox/inboxStore.svelte";
import { touchThread } from "$lib/layout/sessionFocus.svelte";
import { newestLeaf, rootThreadId } from "$lib/thread/messageVersions";
import type { Project, ThreadSummary } from "$lib/types";

/** The tabs of the project detail view. */
export type DetailTab = "overview" | "git" | "worktrees" | "sources" | "integrations" | "usage";

export type View = {
  /** Live thread being viewed. */
  threadId: string | null;
  /** Project context for the sidebar/header. */
  projectPath: string | null;
  /** Draft thread directory (a new, unsent thread). */
  draftCwd: string | null;
  /** Repository whose pull-request review view is open. */
  reviewPath: string | null;
  /** Project whose detail view (overview, git, worktrees, sources) is open. */
  detailPath: string | null;
  /** Which detail tab to land on; `null` means the overview. */
  detailTab: DetailTab | null;
  /** The Memories page is open. */
  memories: boolean;
  /** The Pull requests destination is open, on `inboxPath`'s inbox. */
  pullRequests: boolean;
  /** The repository whose inbox is showing; `null` picks the first. */
  inboxPath: string | null;
  /** The PR the review view opens on, when it was opened for one. */
  reviewPr: number | null;
  /** The repository whose Stack page is open, and GitHub's stack number. */
  stackRepo: string | null;
  stackNumber: number | null;
  /** The PR the Stack page opens on, when it was opened for one. */
  stackPr: number | null;
  /** Start a restack as the Stack page opens (the inbox's Restack). */
  stackRestack: boolean;
  /** `threadId`'s Changes page is open (instead of the thread). */
  changes: boolean;
  /** The file the Changes page opens on. */
  changesPath: string | null;
  /** The Change segment it opens on: a segment id, or a page-made key. */
  changesSegment: string | null;
};

/** How a PR was reached: a `visit` leaves a step for Back to retrace; a `walk`
 *  (j/k through the queue) replaces the PR being read, so Back leaves the queue. */
export type PrMove = "visit" | "walk";

const EMPTY: View = {
  threadId: null,
  projectPath: null,
  draftCwd: null,
  reviewPath: null,
  detailPath: null,
  detailTab: null,
  memories: false,
  pullRequests: false,
  inboxPath: null,
  reviewPr: null,
  stackRepo: null,
  stackNumber: null,
  stackPr: null,
  stackRestack: false,
  changes: false,
  changesPath: null,
  changesSegment: null,
};

export const view = $state<View & { epoch: number }>({
  ...EMPTY,
  /** Bumped on every navigation so keyed views remount with fresh state. */
  epoch: 0,
});

/** Long enough for any real path through the desk. */
const TRAIL_LIMIT = 50;

/** The views Back retraces, latest last. */
let trail = $state.raw<View[]>([]);

function show(patch: Partial<View>, options: { remount?: boolean } = {}): void {
  Object.assign(view, EMPTY, patch);
  if (options.remount !== false) view.epoch += 1;
  // Opening a thread counts as interacting with it for the sidebar's
  // session-focus view.
  touchThread(patch.threadId);
}

/**
 * Show exactly the view described by `patch`; every field it omits is cleared.
 * `remount: false` keeps the epoch (and so the live ThreadView) intact — used
 * when a draft becomes a real thread mid-turn. This is a fresh start, so Back
 * has nothing left to retrace.
 */
export function setView(patch: Partial<View>, options: { remount?: boolean } = {}): void {
  trail = [];
  show(patch, options);
}

/** The current view as Back should return to it: the Stack page reopens on the
 *  PR it last showed (its place), and never restarts a restack. */
function here(): View {
  const { epoch: _epoch, ...current } = view;
  return { ...current, stackPr: null, stackRestack: false, changesPath: null, changesSegment: null };
}

function sameView(a: View, b: View): boolean {
  return (Object.keys(EMPTY) as (keyof View)[]).every((key) => a[key] === b[key]);
}

/** Go to a view Back should return from. */
function visit(patch: Partial<View>): void {
  const from = here();
  if (!sameView(from, { ...EMPTY, ...patch })) trail = [...trail, from].slice(-TRAIL_LIMIT);
  show(patch);
}

/** Back one step: the view left to get here, or else the inbox of the repository
 *  a PR or stack belongs to. */
export function goBack(): void {
  const previous = trail.at(-1);
  if (previous) {
    trail = trail.slice(0, -1);
    show(previous);
    return;
  }
  const repo = view.stackRepo ?? (view.reviewPr !== null ? view.reviewPath : null);
  if (repo) openPullRequests(repo);
  else goHome();
}

/** Names where `goBack` leads, for the button that calls it. */
export function backLabel(): string {
  const previous = trail.at(-1);
  if (!previous) return view.stackRepo || view.reviewPr !== null ? "Pull requests" : "Home";
  if (previous.pullRequests) return "Pull requests";
  if (previous.stackRepo) return "Stack";
  if (previous.reviewPr !== null) return `#${previous.reviewPr}`;
  if (previous.reviewPath) return "Review";
  if (previous.changes) return "Changes";
  if (previous.threadId || previous.draftCwd) return "Thread";
  if (previous.detailPath) return projectByPath(previous.detailPath)?.name ?? "Project";
  return "Home";
}

export function currentProject(): Project | null {
  return projectByPath(view.projectPath);
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

const branches = () => appData.data?.threadBranches ?? [];

/** An edited message leaves its thread as a family of versions; the sidebar
 *  lists only the root, and opening it lands on whichever was active last. */
function newestVersion(threadId: string): string {
  return newestLeaf(threadId, branches(), (id) => threadById(id)?.updatedAt);
}

/** Open a thread from the sidebar, where the project is already known. */
export function openThread(project: Project, threadId: string): void {
  setView({ projectPath: project.path, threadId: newestVersion(threadId) });
}

/** Open a thread by id, resolving its project from the thread's cwd. A
 *  version branch resolves through its root, which is the listed one. */
export function openThreadById(threadId: string): void {
  const thread = threadById(threadId) ?? threadById(rootThreadId(threadId, branches()) ?? "");
  if (!thread) return;
  setView({ threadId: newestVersion(threadId), projectPath: projectForCwd(thread.cwd)?.path ?? view.projectPath });
}

/** Open a specific version of an edited message's thread, as it is. */
export function openVersion(threadId: string): void {
  setView({ threadId, projectPath: view.projectPath });
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
  const start = inProject ? null : pageThreadStart();
  if (start) {
    void start();
    return;
  }
  const project = inProject ?? currentProject() ?? projects().find((candidate) => !candidate.archived) ?? null;
  if (!project) return;
  setView({ projectPath: project.path, draftCwd: project.path });
}

/** Start a draft thread whose cwd is an exact directory (e.g. a worktree). */
export function newThreadInDir(cwd: string): void {
  setView({ draftCwd: cwd, projectPath: projectForCwd(cwd)?.path ?? view.projectPath });
}

export function openProjectDetail(project: Project, tab: DetailTab = "overview"): void {
  setView({ projectPath: project.path, detailPath: project.path, detailTab: tab });
}

/** The Worktrees tab of a project's details. */
export function openWorktrees(project: Project): void {
  openProjectDetail(project, "worktrees");
}

/** The Git tab of a project's details. */
export function openGit(project: Project): void {
  openProjectDetail(project, "git");
}

/** The review view, on a PR or on its picker. A PR in a stack opens on the Stack
 *  page, which shows the stack beside it. */
export function openReview(project: Project, prNumber: number | null = null, move: PrMove = "visit"): void {
  const stack = prNumber === null ? null : stackHolding(project.path, prNumber);
  if (stack) lastInboxPath = project.path;
  const patch: Partial<View> = stack
    ? { projectPath: view.projectPath, stackRepo: project.path, stackNumber: stack.number, stackPr: prNumber }
    : { projectPath: project.path, reviewPath: project.path, reviewPr: prNumber };
  if (move === "walk") show(patch);
  else visit(patch);
}

/** The inbox the Pull requests destination last showed. */
let lastInboxPath: string | null = null;

/** The Pull requests destination: `path`'s inbox, else the one shown last,
 *  else the current project's. */
export function openPullRequests(path: string | null = null): void {
  const inboxPath = path ?? lastInboxPath ?? view.projectPath;
  lastInboxPath = inboxPath;
  setView({ projectPath: view.projectPath, pullRequests: true, inboxPath });
}

/** The Stack page for GitHub stack `number` in `repoDir`; `restack` starts
 *  a restack as it opens. */
export function openStackPage(repoDir: string, number: number, options: { restack?: boolean } = {}): void {
  lastInboxPath = repoDir;
  visit({
    projectPath: view.projectPath,
    stackRepo: repoDir,
    stackNumber: number,
    stackRestack: options.restack ?? false,
  });
}

/** The Memories page, keeping the current project in the sidebar. */
/** A thread's Changes page, opening on `path` or a segment when given. Back
 *  returns to wherever it was opened from (the thread, usually). */
export function openChanges(threadId: string, options: { path?: string | null; segment?: string | null } = {}): void {
  const thread = threadById(threadId);
  visit({
    threadId,
    projectPath: view.threadId === threadId ? view.projectPath : (projectForCwd(thread?.cwd ?? null)?.path ?? null),
    changes: true,
    changesPath: options.path ?? null,
    changesSegment: options.segment ?? null,
  });
}

export function openMemories(): void {
  setView({ projectPath: view.projectPath, memories: true });
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
