/**
 * Everything the sidebar/home context menus and the thread views can ask the
 * app to do: rename, archive, delete, pin, reorder, fork, workspace moves.
 * Actions that need confirmation or input raise a dialog and act on its result.
 */
import {
  appData,
  applyData,
  fail,
  projects,
  quietRefresh,
  trackNewThread,
  trackSubagent,
} from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import {
  adoptThread,
  clearThread,
  goHome,
  newThread,
  newThreadInDir,
  openProjectDetail,
  openThreadInCwd,
  setView,
  view,
} from "$lib/app/navigation.svelte";
import type { SlashCommandId } from "$lib/composer/slashCommands";
import DeleteThreadDialog from "$lib/layout/DeleteThreadDialog.svelte";
import RenameDialog from "$lib/layout/RenameDialog.svelte";
import SectionPickerDialog from "$lib/layout/SectionPickerDialog.svelte";
import {
  archiveThread,
  createThreadSection,
  createWorkspace,
  deleteThread,
  deleteThreadSection,
  forkThread,
  moveProject,
  moveThreadToSection,
  moveThreadToWorkspace,
  removeProject,
  renameProject,
  renameThread,
  revealInFinder,
  saveDraft,
  setProjectArchived,
  setProjectPinned,
  setThreadPinned,
  updateThreadSection,
  updateWorkspace,
} from "$lib/services/api";
import type { CreateWorkspaceInput, MenuAction, MenuTarget, Project, SubagentDetail } from "$lib/types";
import CreateWorkspaceDialog from "$lib/workspaces/CreateWorkspaceDialog.svelte";
import MoveToWorkspaceDialog from "$lib/workspaces/MoveToWorkspaceDialog.svelte";

/** A draft thread has become a real one: keep it on screen and let the
 *  sidebar catch up in the background. */
export function threadCreated(id: string, cwd: string): void {
  trackNewThread(id, cwd);
  adoptThread(id);
  quietRefresh();
}

export function openSubagent(agent: SubagentDetail): void {
  trackSubagent(agent);
  // App-owned agents run in threads deliberately hidden from the sidebar, and
  // `openThreadById` silently does nothing for a thread it cannot find there.
  openThreadInCwd(agent.id, agent.cwd);
}

/** "Ask Codex to review": open a draft thread in the repo cwd with the review
 *  prompt pre-filled in the composer (via the per-project draft). */
export async function askCodexReview(cwd: string, prompt: string): Promise<void> {
  try {
    await saveDraft(cwd, JSON.stringify([{ type: "text", text: prompt }]));
  } catch {
    // A failed draft save just leaves the composer empty; still open the thread.
  }
  newThreadInDir(cwd);
}

export function slashCommand(command: SlashCommandId, threadId: string | null): void {
  if (command === "new") {
    newThread();
    return;
  }
  // `compact` is handled inside ThreadView, which owns the live thread.
  if (command !== "fork" && command !== "archive" && command !== "rename" && command !== "delete") return;
  if (!threadId) return;
  for (const project of projects()) {
    const thread = project.threads.find((candidate) => candidate.id === threadId);
    if (thread) {
      menuAction(command, { kind: "thread", project, thread });
      return;
    }
  }
}

async function rename(target: MenuTarget) {
  const current =
    target.kind === "project" ? target.project.name : target.kind === "thread" ? target.thread.title : target.section.name;
  const name = await openDialog(RenameDialog, { kind: target.kind, current });
  if (!name) return;
  appData.loading = true;
  try {
    applyData(
      target.kind === "project"
        ? await renameProject(target.project.path, name)
        : target.kind === "thread"
          ? await renameThread(target.thread.id, name)
          : await updateThreadSection(target.section.id, name, target.section.color ?? null),
    );
  } catch (cause) {
    fail(cause);
  } finally {
    appData.loading = false;
  }
}

/** Pick (or create) a section for a thread. Creating one moves the thread
 *  into it in the same step: a second round trip after the create resolves
 *  the new id from the refreshed section list. */
async function moveToSection(threadId: string, currentSectionId: string | null | undefined) {
  const choice = await openDialog(SectionPickerDialog, {
    sections: appData.data?.sections ?? [],
    current: currentSectionId ?? null,
  });
  if (!choice) return;
  if (choice.kind === "existing") {
    applyData(await moveThreadToSection(threadId, choice.section.id));
    return;
  }
  if (choice.kind === "none") {
    applyData(await moveThreadToSection(threadId, null));
    return;
  }
  const before = new Set((appData.data?.sections ?? []).map((section) => section.id));
  const created = await createThreadSection(choice.name, choice.color);
  applyData(created);
  const section = (created.sections ?? []).find((section) => !before.has(section.id));
  if (section) applyData(await moveThreadToSection(threadId, section.id));
}

async function confirmDelete(target: MenuTarget) {
  if (target.kind !== "thread") return;
  if (!(await openDialog(DeleteThreadDialog, { title: target.thread.title }))) return;
  applyData(await deleteThread(target.thread.id));
  clearThread(target.thread.id);
}

async function moveToWorkspace(threadId: string) {
  const workspace = await openDialog(MoveToWorkspaceDialog, { workspaces: projects() });
  if (!workspace?.workspaceId) return;
  const next = await moveThreadToWorkspace(threadId, workspace.workspaceId);
  applyData(next);
  setView({
    threadId,
    projectPath: (next.projects.find((project) => project.workspaceId === workspace.workspaceId) ?? workspace).path,
  });
}

export async function menuAction(action: MenuAction, target: MenuTarget): Promise<void> {
  try {
    if (
      target.kind === "project" &&
      target.project.kind === "multiProject" &&
      !["reveal", "openDetails"].includes(action)
    ) {
      return;
    }
    if (action === "openDetails") {
      if (target.kind === "project") openProjectDetail(target.project);
      return;
    }
    if (action === "toggleArchive") {
      if (target.kind !== "project") return;
      applyData(await setProjectArchived(target.project.path, !target.project.archived));
      if (view.projectPath === target.project.path && !target.project.archived) goHome();
      return;
    }
    if (action === "remove") {
      if (target.kind !== "project") return;
      applyData(await removeProject(target.project.path));
      if (view.projectPath === target.project.path) goHome();
      return;
    }
    if (action === "moveUp" || action === "moveDown") {
      if (target.kind !== "project") return;
      applyData(await moveProject(target.project.path, action === "moveUp" ? -1 : 1));
      return;
    }
    if (action === "fork") {
      if (target.kind !== "thread") return;
      const forked = await forkThread(target.thread.id);
      setView({ projectPath: target.project.path, threadId: forked.id });
      quietRefresh();
      return;
    }
    if (action === "moveToWorkspace") {
      if (target.kind === "thread") await moveToWorkspace(target.thread.id);
      return;
    }
    if (action === "moveToSection") {
      if (target.kind === "thread") await moveToSection(target.thread.id, target.thread.sectionId);
      return;
    }
    if (action === "deleteSection") {
      if (target.kind === "section") applyData(await deleteThreadSection(target.section.id));
      return;
    }
    if (target.kind === "section") {
      if (action === "rename") await rename(target);
      return;
    }
    if (action === "reveal") {
      await revealInFinder(target.kind === "project" ? target.project.path : target.thread.cwd || target.project.path);
      return;
    }
    if (action === "rename") {
      await rename(target);
      return;
    }
    if (action === "archive") {
      if (target.kind !== "thread") return;
      applyData(await archiveThread(target.thread.id));
      clearThread(target.thread.id);
      return;
    }
    if (action === "delete") {
      await confirmDelete(target);
      return;
    }
    if (target.kind === "project") {
      applyData(await setProjectPinned(target.project.path, !target.project.pinned));
    } else {
      applyData(await setThreadPinned(target.thread.id, !target.thread.pinned));
    }
  } catch (cause) {
    fail(cause);
  }
}

/** Rename a worktree's project entry from the Worktrees view. */
export function renameProjectAt(path: string): void {
  const project = projects().find((candidate) => candidate.path === path);
  if (project) menuAction("rename", { kind: "project", project });
}

/** Create a workspace, or edit an existing one when `workspace` is given. */
export function openWorkspaceDialog(workspace: Project | null = null): void {
  openDialog(CreateWorkspaceDialog, {
    projects: projects(),
    workspace,
    submit: async (input: CreateWorkspaceInput) => {
      const next = workspace?.workspaceId
        ? await updateWorkspace(workspace.workspaceId, input)
        : await createWorkspace(input);
      applyData(next);
      const saved = workspace?.workspaceId
        ? next.projects.find((project) => project.workspaceId === workspace.workspaceId)
        : next.projects.find((project) => project.kind === "multiProject" && project.name === input.name);
      if (saved) openProjectDetail(saved);
    },
  });
}
