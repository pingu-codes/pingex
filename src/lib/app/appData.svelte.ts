/**
 * The bootstrapped Codex data (projects, threads, account) shared by the app
 * shell. Owns the optimistic entries that keep freshly created threads and
 * subagents visible until a bootstrap refresh catches up with them.
 */

import { checkCodexVersion } from "$lib/app/codexVersion.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import AddProjectDialog from "$lib/layout/AddProjectDialog.svelte";
import { seedActiveTurns, setUnansweredQuestions } from "$lib/services/codexEvents.svelte";
import { addProfileProject, commands } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";
import type { BootstrapData, Host, Project, SubagentDetail, ThreadSummary } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

export const appData = $state<{
  data: BootstrapData | null;
  loading: boolean;
  error: string | null;
}>({
  data: null,
  loading: true,
  error: null,
});

export function projects(): Project[] {
  return appData.data?.projects ?? [];
}

/** Every thread the sidebar knows about, including subagent threads. */
export function threads(): ThreadSummary[] {
  return [...projects().flatMap((project) => project.threads), ...(appData.data?.subagents ?? [])];
}

export function threadById(threadId: string | null): ThreadSummary | null {
  if (!threadId) return null;
  return threads().find((thread) => thread.id === threadId) ?? null;
}

export function projectByPath(path: string | null): Project | null {
  if (!path) return null;
  return projects().find((project) => project.path === path) ?? null;
}

// Compare path components without rewriting the paths used for IPC or display.
// Keep case intact: native and WSL filesystems may be case-sensitive.
function directoryKey(path: string): string {
  return path.replaceAll("\\", "/").replace(/\/+$/, "") || "/";
}

/**
 * The project a directory belongs to (exact match, else nearest ancestor).
 *
 * A temporary worktree lives outside every project path, so the listing itself
 * is the last word: whichever project already holds a thread running there is
 * the project that thread belongs to.
 */
export function projectForDirectory(candidates: Project[], cwd: string): Project | null {
  const directory = directoryKey(cwd);
  let nearest: Project | null = null;
  let nearestLength = -1;
  for (const project of candidates) {
    const root = directoryKey(project.path);
    if (directory === root) return project;
    if (directory.startsWith(root.endsWith("/") ? root : `${root}/`) && root.length > nearestLength) {
      nearest = project;
      nearestLength = root.length;
    }
  }
  return (
    nearest ??
    candidates.find((project) => project.threads.some((thread) => directoryKey(thread.cwd) === directory)) ??
    null
  );
}

export function projectForCwd(cwd: string | null): Project | null {
  return cwd ? projectForDirectory(projects(), cwd) : null;
}

export function fail(cause: unknown): void {
  appData.error = errorMessage(cause);
}

/** The title a thread carries until it has a message to be named after. */
export const UNTITLED_THREAD = "Untitled thread";

/** Threads shown in the sidebar before their rollout is persisted on disk.
 *  bootstrap() only sees persisted rollouts, so without this a new thread
 *  would vanish from (or never reach) the sidebar until the turn completes. */
const optimisticThreads = new Map<string, ThreadSummary>();

/** Subagent threads opened before bootstrap knows them (e.g. spawned mid plan
 *  mode). Kept in `data.subagents` across refreshes until bootstrap catches up
 *  so the header title/back button and re-selection keep working. */
const optimisticSubagents = new Map<string, ThreadSummary>();

function insertOptimisticThread(target: BootstrapData, summary: ThreadSummary) {
  const project = projectForDirectory(target.projects, summary.cwd);
  if (!project || project.threads.some((thread) => thread.id === summary.id)) return;
  const index = project.threads.findIndex((thread) => !thread.pinned);
  project.threads.splice(index === -1 ? project.threads.length : index, 0, summary);
}

/**
 * Sidebar data arrives from many requests at once: refreshes after events,
 * action results, auto-naming. Each takes a ticket when it starts, and a
 * response older than one already applied is dropped, so a slow refresh can
 * no longer undo a project that was just added. A dropped action result may
 * still hold a change the applied one predates, so it triggers one fresh
 * read to converge.
 */
let dataRequested = 0;
let dataApplied = 0;

/** Take a ticket for a request about to replace the sidebar data. */
export function dataTicket(): number {
  return ++dataRequested;
}

/** Apply `next` unless a request that started later already landed. */
export function applyTicketed(next: BootstrapData, ticket: number, reconcile = true): boolean {
  if (ticket < dataApplied) {
    if (reconcile) void quietRefresh();
    return false;
  }
  dataApplied = ticket;
  applyData(next);
  return true;
}

/** `applyData(await request)`, in request order. Pass the promise straight
 *  from the call so the ticket is taken as the request starts. */
export async function applyFresh(request: Promise<BootstrapData>): Promise<BootstrapData> {
  const ticket = dataTicket();
  const next = await request;
  applyTicketed(next, ticket);
  return next;
}

export function applyData(next: BootstrapData): void {
  for (const [id, summary] of optimisticThreads) {
    if (next.projects.some((project) => project.threads.some((thread) => thread.id === id))) {
      optimisticThreads.delete(id);
    } else {
      insertOptimisticThread(next, summary);
    }
  }
  for (const [id, summary] of optimisticSubagents) {
    if (next.subagents.some((thread) => thread.id === id)) {
      optimisticSubagents.delete(id);
    } else {
      next.subagents.push(summary);
    }
  }
  appData.data = next;
}

/** Stop showing a thread that was never persisted (deleted or archived before
 *  bootstrap reported it). Without this, `applyData` keeps re-inserting it. */
export function forgetOptimisticThread(id: string): void {
  optimisticThreads.delete(id);
  optimisticSubagents.delete(id);
}

/** Register a brand-new thread so the sidebar shows it before it persists. */
export function trackNewThread(id: string, cwd: string, inTempWorktree = false): void {
  const summary: ThreadSummary = {
    id,
    cwd,
    title: UNTITLED_THREAD,
    updatedAt: Math.floor(Date.now() / 1000),
    status: "idle",
    pinned: false,
    parentThreadId: null,
    agentNickname: null,
    agentRole: null,
    projectId: null,
    sectionId: null,
    subagentCount: 0,
    hidden: false,
    inTempWorktree,
  };
  optimisticThreads.set(id, summary);
  if (appData.data) insertOptimisticThread(appData.data, summary);
}

/**
 * Title a just-created thread from the message that opened it.
 *
 * Nothing else can name it for a while: bootstrap does not see a thread until
 * its rollout is on disk, and the auto-namer costs a Codex round trip — so
 * without this the sidebar reads "Untitled thread" for as long as the first
 * turn runs. Only the placeholder is overwritten, so a real title that has
 * already landed is never lost to a late call.
 */
export function nameNewThread(id: string, title: string): void {
  const tracked = optimisticThreads.get(id);
  const shown = threadById(id);
  // Both hold the same summary once it has been inserted, so the placeholder is
  // checked once — testing them in turn would let the first write mask the
  // second and leave the sidebar's copy unnotified.
  if (!title || (shown ?? tracked)?.title !== UNTITLED_THREAD) return;
  if (shown) shown.title = title;
  if (tracked) tracked.title = title;
}

/** Register a subagent thread bootstrap has not reported yet. */
export function trackSubagent(agent: SubagentDetail): void {
  const known = threads().some((thread) => thread.id === agent.id);
  if (known || !appData.data) return;
  const summary: ThreadSummary = {
    id: agent.id,
    cwd: agent.cwd,
    title: agent.title,
    updatedAt: Math.floor(Date.now() / 1000),
    status: agent.status,
    pinned: false,
    projectId: null,
    sectionId: null,
    subagentCount: 0,
    hidden: false,
    inTempWorktree: false,
    parentThreadId: agent.parentThreadId,
    agentNickname: agent.agentNickname,
    agentRole: agent.agentRole,
  };
  optimisticSubagents.set(agent.id, summary);
  appData.data.subagents.push(summary);
}

export async function refresh(): Promise<void> {
  appData.loading = true;
  appData.error = null;
  try {
    await applyFresh(commands.bootstrap());
    // Questions the app never got to answer only exist in the database, so the
    // sidebar has to be told about them explicitly.
    commands
      .threadsWithUnansweredQuestions()
      .then(setUnansweredQuestions)
      .catch(() => {});
    // A turn that began before this webview loaded (a reload mid-turn) never
    // sends `turn/started` again; the backend still knows it is running.
    commands
      .threadsWithActiveTurns()
      .then(seedActiveTurns)
      .catch(() => {});
    // The child is up by now, so this reads what it reported at `initialize`
    // rather than spawning one of its own.
    void checkCodexVersion();
  } catch (cause) {
    fail(cause);
  } finally {
    appData.loading = false;
  }
}

/** Refresh without the spinner; used after background changes. */
export async function quietRefresh(): Promise<void> {
  const ticket = dataTicket();
  try {
    // A stale refresh needs no follow-up: a newer read already landed.
    applyTicketed(await commands.bootstrap(), ticket, false);
  } catch {
    // Sidebar refresh after thread creation is best-effort.
  }
}

export async function addProject(): Promise<void> {
  if (!isTauri()) return;
  await openDialog<true, { submit: (path: string, host: Host | null) => Promise<void> }>(AddProjectDialog, {
    submit: async (path, host) => {
      await applyFresh(addProfileProject(path, host));
    },
  });
}
