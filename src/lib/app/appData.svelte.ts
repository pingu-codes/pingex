/**
 * The bootstrapped Codex data (projects, threads, account) shared by the app
 * shell. Owns the optimistic entries that keep freshly created threads and
 * subagents visible until a bootstrap refresh catches up with them.
 */
import { open } from "@tauri-apps/plugin-dialog";
import { bootstrap, isTauri, saveProject, threadsWithUnansweredQuestions } from "$lib/services/api";
import { setUnansweredQuestions } from "$lib/services/codexEvents.svelte";
import type { BootstrapData, Project, SubagentDetail, ThreadSummary } from "$lib/types";

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

/**
 * The project a directory belongs to (exact match, else nearest ancestor).
 *
 * A temporary worktree lives outside every project path, so the listing itself
 * is the last word: whichever project already holds a thread running there is
 * the project that thread belongs to.
 */
export function projectForCwd(cwd: string | null): Project | null {
  if (!cwd) return null;
  return (
    projects().find((project) => project.path === cwd) ??
    projects().find((project) => cwd.startsWith(project.path)) ??
    projects().find((project) => project.threads.some((thread) => thread.cwd === cwd)) ??
    null
  );
}

export function fail(cause: unknown): void {
  appData.error = cause instanceof Error ? cause.message : String(cause);
}

/** Threads shown in the sidebar before their rollout is persisted on disk.
 *  bootstrap() only sees persisted rollouts, so without this a new thread
 *  would vanish from (or never reach) the sidebar until the turn completes. */
const optimisticThreads = new Map<string, ThreadSummary>();

/** Subagent threads opened before bootstrap knows them (e.g. spawned mid plan
 *  mode). Kept in `data.subagents` across refreshes until bootstrap catches up
 *  so the header title/back button and re-selection keep working. */
const optimisticSubagents = new Map<string, ThreadSummary>();

function insertOptimisticThread(target: BootstrapData, summary: ThreadSummary) {
  const project = target.projects.find((candidate) => summary.cwd.startsWith(candidate.path));
  if (!project || project.threads.some((thread) => thread.id === summary.id)) return;
  const index = project.threads.findIndex((thread) => !thread.pinned);
  project.threads.splice(index === -1 ? project.threads.length : index, 0, summary);
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

/** Register a brand-new thread so the sidebar shows it before it persists. */
export function trackNewThread(id: string, cwd: string): void {
  const summary: ThreadSummary = {
    id,
    cwd,
    title: "Untitled thread",
    updatedAt: Math.floor(Date.now() / 1000),
    status: "idle",
    pinned: false,
  };
  optimisticThreads.set(id, summary);
  if (appData.data) insertOptimisticThread(appData.data, summary);
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
    applyData(await bootstrap());
    // Questions the app never got to answer only exist in the database, so the
    // sidebar has to be told about them explicitly.
    threadsWithUnansweredQuestions()
      .then(setUnansweredQuestions)
      .catch(() => {});
  } catch (cause) {
    fail(cause);
  } finally {
    appData.loading = false;
  }
}

/** Refresh without the spinner; used after background changes. */
export async function quietRefresh(): Promise<void> {
  try {
    applyData(await bootstrap());
  } catch {
    // Sidebar refresh after thread creation is best-effort.
  }
}

export async function addProject(): Promise<void> {
  if (!isTauri()) return;
  const path = await open({ directory: true, multiple: false, title: "Add project folder" });
  if (!path) return;
  appData.loading = true;
  try {
    applyData(await saveProject(path));
  } catch (cause) {
    fail(cause);
  } finally {
    appData.loading = false;
  }
}
