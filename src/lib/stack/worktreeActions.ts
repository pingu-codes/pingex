// The sidebar worktree menu's actions (Context menus board, "Worktree in the
// sidebar", story 115): New thread here, Switch to PR, Pull latest, Copy
// path, Archive its threads and Remove worktree…. Open in is the menu's
// existing "Open in Finder".

import { quietRefresh } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import { newThreadInDir } from "$lib/app/navigation.svelte";
import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { prLinks } from "$lib/review/prLinks.svelte";
import { commands } from "$lib/services/homeRouting";
import SwitchToPrDialog from "$lib/stack/SwitchToPrDialog.svelte";
import { stackInputFor } from "$lib/stack/stack";
import { toastError, toaster, toastFailure, toastProgress } from "$lib/toaster";
import type { Project, StackInputPr, WorktreeEntry } from "$lib/types";
import { plural } from "$lib/utils/format";
import { basename, trimSlash } from "$lib/utils/path";
import RemoveWorktreeDialog from "$lib/worktrees/RemoveWorktreeDialog.svelte";
import { worktreeMap } from "$lib/worktrees/worktreeMap.svelte";

export type WorktreeAction =
  | "newThreadHere"
  | "switchToPr"
  | "pullLatest"
  | "copyPath"
  | "archiveThreads"
  | "removeWorktree";

/** How long Switch to PR waits between looks at the running threads. */
const WAIT_MS = 2000;

/** The repository a worktree belongs to, and its `git worktree list` entry. */
function worktreeOf(path: string): { repo: string; entry: WorktreeEntry } | null {
  const wanted = trimSlash(path);
  for (const [repo, entries] of Object.entries(worktreeMap.byRepo)) {
    const entry = entries.find((candidate) => trimSlash(candidate.path) === wanted);
    if (entry) return { repo, entry };
  }
  return null;
}

/** The open PRs of the stack whose git branch is checked out at `branch`. */
export function stackAt(repo: string, branch: string | null): StackInputPr[] {
  const state = inboxes[repo];
  if (!branch || !state?.inbox || !state.stacks) return [];
  for (const stack of state.stacks.stacks) {
    if (!stack.prs.some((pr) => pr.headRef === branch)) continue;
    return stackInputFor(stack, state.inbox).prs.filter((pr) => pr.open && !pr.merged);
  }
  return [];
}

async function runningIn(project: Project): Promise<string[]> {
  const active = new Set(await commands.threadsWithActiveTurns());
  return project.threads.filter((thread) => active.has(thread.id)).map((thread) => thread.id);
}

async function switchToPr(project: Project) {
  const found = worktreeOf(project.path);
  const prs = found ? stackAt(found.repo, found.entry.branch) : [];
  if (!found || prs.length === 0) {
    toastError("This worktree isn't on a stack's git branch");
    return;
  }
  const running = await runningIn(project).catch(() => [] as string[]);
  const branch = await openDialog(SwitchToPrDialog, {
    name: basename(project.path),
    prs,
    current: found.entry.branch,
    running: running.map((id) => ({ prNumber: prLinks.byThread[id]?.prNumber ?? null })),
  });
  if (!branch) return;
  let waiting: string | null = null;
  let stopped = false;
  // A failed read says nothing, so it keeps waiting: switching under a running
  // thread is what this is here to avoid.
  while (!stopped && (await runningIn(project).catch(() => ["unknown"])).length > 0) {
    waiting ??= toastProgress(`Waiting to switch ${basename(project.path)}`, "Threads are still running here", {
      label: "Stop waiting",
      onClick: () => (stopped = true),
    });
    await new Promise((resolve) => setTimeout(resolve, WAIT_MS));
  }
  if (waiting) toaster.dismiss(waiting);
  if (stopped) return;
  try {
    await commands.gitCheckoutBranch(project.path, branch, false);
    toaster.success({ title: `${basename(project.path)} is on ${branch}` });
  } catch (cause) {
    toastFailure(cause);
  }
}

export async function worktreeAction(action: WorktreeAction, project: Project): Promise<void> {
  switch (action) {
    case "newThreadHere":
      return newThreadInDir(project.path);
    case "switchToPr":
      return switchToPr(project);
    case "pullLatest": {
      const result = await commands.gitPull(project.path);
      toaster.success({ title: `Pulled ${basename(project.path)}`, description: result.summary });
      return;
    }
    case "copyPath":
      await commands.copyText(project.path);
      return;
    case "archiveThreads": {
      const threads = project.threads;
      if (threads.length === 0) return;
      const ok = await openDialog(ConfirmDialog, {
        title: `Archive ${plural(threads.length, "thread")}`,
        body: `Archive every thread in ${basename(project.path)}. They stay in the archive.`,
        confirmLabel: "Archive threads",
        tone: "warning" as const,
      });
      if (!ok) return;
      for (const thread of threads) await commands.archiveThread(thread.id);
      await quietRefresh();
      return;
    }
    case "removeWorktree": {
      const found = worktreeOf(project.path);
      if (!found) {
        toastError("Pingex can't find this worktree's repository");
        return;
      }
      const result = await openDialog(RemoveWorktreeDialog, {
        entry: found.entry,
        threadCount: project.threads.length,
      });
      if (!result) return;
      await commands.gitWorktreeRemove(found.repo, project.path, result.force);
      await quietRefresh();
      return;
    }
  }
}
