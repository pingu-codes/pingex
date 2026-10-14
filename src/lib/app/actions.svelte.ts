/**
 * Everything the sidebar/home context menus and the thread views can ask the
 * app to do: rename, archive, delete, pin, reorder, fork, workspace moves.
 * Actions that need confirmation or input raise a dialog and act on its result.
 */
import {
  appData,
  applyFresh,
  applyTicketed,
  dataTicket,
  fail,
  forgetOptimisticThread,
  projects,
  quietRefresh,
  trackNewThread,
  trackSubagent,
} from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import { codexHome } from "$lib/app/launch.svelte";
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
import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
import type { SlashCommandId } from "$lib/composer/slashCommands";
import RenameDialog from "$lib/layout/RenameDialog.svelte";
import SectionPickerDialog from "$lib/layout/SectionPickerDialog.svelte";
import { touchThread } from "$lib/layout/sessionFocus.svelte";
import { isEmptyPlan, sessionFocusPlan } from "$lib/layout/sessionFocusPlan";
import {
  buildTree,
  childrenOf,
  emptyLayout,
  parentOf,
  placeInLayout,
  ROOT_SCOPE,
  refOf,
} from "$lib/layout/sidebarTree";
import { forkThread, gitWorktreeAdd, updateWorkspace } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import type {
  CreateWorkspaceInput,
  MenuAction,
  MenuTarget,
  Project,
  SidebarItemRef,
  SubagentDetail,
  WorktreeBranchRequest,
} from "$lib/types";
import CreateWorkspaceDialog from "$lib/workspaces/CreateWorkspaceDialog.svelte";
import MoveToWorkspaceDialog from "$lib/workspaces/MoveToWorkspaceDialog.svelte";
import AddWorktreeDialog from "$lib/worktrees/AddWorktreeDialog.svelte";
import CreateWorktreeDialog from "$lib/worktrees/CreateWorktreeDialog.svelte";

/** A draft thread has become a real one: keep it on screen and let the
 *  sidebar catch up in the background. */
export function threadCreated(id: string, cwd: string): void {
  touchThread(id);
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
    await commands.saveDraft(cwd, JSON.stringify([{ type: "text", text: prompt }]));
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
  // `compact` is handled inside ThreadView, against the thread's live session.
  if (command !== "fork" && command !== "archive" && command !== "rename" && command !== "delete") return;
  if (!threadId) return;
  for (const project of projects()) {
    const thread = project.threads.find((candidate) => candidate.id === threadId);
    if (thread) {
      menuAction(command, { kind: "thread", project, thread, order: project.threads.map((t) => t.id) });
      return;
    }
  }
}

async function rename(target: MenuTarget) {
  const current =
    target.kind === "project"
      ? target.project.name
      : target.kind === "thread"
        ? target.thread.title
        : target.kind === "folder"
          ? target.folder.name
          : target.section.name;
  const name = await openDialog(RenameDialog, { kind: target.kind, current });
  if (!name) return;
  appData.loading = true;
  try {
    await applyFresh(
      target.kind === "project"
        ? commands.renameProject(target.project.path, name)
        : target.kind === "thread"
          ? commands.renameThread(target.thread.id, name)
          : target.kind === "folder"
            ? commands.renameSidebarFolder(target.folder.id, name)
            : commands.updateThreadSection(target.section.id, name, target.section.color ?? null),
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
    await applyFresh(commands.moveThreadToSection(threadId, choice.section.id));
    return;
  }
  if (choice.kind === "none") {
    await applyFresh(commands.moveThreadToSection(threadId, null));
    return;
  }
  const before = new Set((appData.data?.sections ?? []).map((section) => section.id));
  const created = await applyFresh(commands.createThreadSection(choice.name, choice.color));
  const section = (created.sections ?? []).find((section) => !before.has(section.id));
  if (section) await applyFresh(commands.moveThreadToSection(threadId, section.id));
}

async function confirmDelete(target: MenuTarget) {
  if (target.kind !== "thread") return;
  const confirmed = await openDialog(ConfirmDialog, {
    title: "Delete thread",
    body: `Permanently delete “${target.thread.title}”? This cannot be undone.`,
    confirmLabel: "Delete",
  });
  if (!confirmed) return;
  const ticket = dataTicket();
  const deleted = await commands.deleteThread(target.thread.id);
  forgetOptimisticThread(target.thread.id);
  applyTicketed(deleted, ticket);
  clearThread(target.thread.id);
}

async function moveToWorkspace(threadId: string) {
  const workspace = await openDialog(MoveToWorkspaceDialog, { workspaces: projects() });
  if (!workspace?.workspaceId) return;
  const next = await applyFresh(commands.moveThreadToWorkspace(threadId, workspace.workspaceId));
  setView({
    threadId,
    projectPath: (next.projects.find((project) => project.workspaceId === workspace.workspaceId) ?? workspace).path,
  });
}

/** Ask for a name and create a sidebar folder under `parentId` in `scope`. */
export async function newFolder(scope: string, parentId: string | null): Promise<void> {
  const name = await openDialog(RenameDialog, {
    kind: "folder" as const,
    current: "",
    title: parentId ? "New subfolder" : "New folder",
    submitLabel: "Create",
  });
  if (!name) return;
  try {
    await applyFresh(commands.createSidebarFolder(scope, parentId, name));
  } catch (cause) {
    fail(cause);
  }
}

/** A drag-and-drop landed: store the new parent and sibling order. */
export async function moveSidebarItem(
  scope: string,
  item: SidebarItemRef,
  parentId: string | null,
  siblings: SidebarItemRef[],
): Promise<void> {
  const data = appData.data;
  if (!data) return;
  const previousLayout = data.sidebarLayout;
  data.sidebarLayout = placeInLayout(previousLayout ?? emptyLayout(), scope, item, parentId, siblings);
  try {
    await applyFresh(commands.placeSidebarItem(scope, item, parentId, siblings));
  } catch (cause) {
    data.sidebarLayout = previousLayout;
    fail(cause);
  }
}

/** Forget the user's drag ordering in `scope` (`ROOT_SCOPE` for projects). */
export async function resetOrder(scope: string): Promise<void> {
  try {
    await applyFresh(commands.resetSidebarOrder(scope));
  } catch (cause) {
    fail(cause);
  }
}

/** The session-focus button. */
export async function focusSession(): Promise<void> {
  const plan = sessionFocusPlan(projects(), appData.data?.sidebarLayout ?? emptyLayout(), view.threadId);
  if (isEmptyPlan(plan)) return;
  try {
    await applyFresh(commands.applySessionFocus(plan.hide, plan.collapseProjects, plan.collapseFolders));
  } catch (cause) {
    fail(cause);
  }
}

/** "Move up/down" for a project: swap it with its neighbour among its
 *  siblings. Pinned projects always float above unpinned ones, so a swap
 *  across that boundary would be invisible and is skipped. */
async function nudgeProject(project: Project, direction: -1 | 1) {
  const visible = projects().filter((candidate) => !candidate.archived);
  const tree = buildTree(appData.data?.sidebarLayout ?? { folders: [], placements: [] }, ROOT_SCOPE, visible, {
    key: (candidate: Project) => candidate.path,
    pinned: (candidate: Project) => candidate.pinned,
  });
  const ref: SidebarItemRef = { kind: "item", id: project.path };
  const parent = parentOf(tree, ref);
  if (parent === undefined) return;
  const siblings = (childrenOf(tree, parent) ?? []).map(refOf);
  const index = siblings.findIndex((sibling) => sibling.id === ref.id && sibling.kind === "item");
  const target = index + direction;
  if (index < 0 || target < 0 || target >= siblings.length) return;
  const neighbour = siblings[target];
  if (
    neighbour.kind === "item" &&
    visible.find((candidate) => candidate.path === neighbour.id)?.pinned !== project.pinned
  ) {
    return;
  }
  [siblings[index], siblings[target]] = [siblings[target], siblings[index]];
  await moveSidebarItem(ROOT_SCOPE, ref, parent, siblings);
}

/** Adopt an existing linked worktree of `project`'s repository. */
async function addWorktreeFor(project: Project): Promise<void> {
  const repoDir = (await commands.gitRepoInfo(project.path).catch(() => null))?.root ?? project.path;
  openDialog(AddWorktreeDialog, {
    repoDir,
    projects: projects(),
    submit: async (path: string) => {
      await applyFresh(commands.addWorktreeProject(path));
    },
  });
}

/** Create a permanent worktree for the repository containing `dir`. */
async function createWorktreeFor(dir: string): Promise<void> {
  const repoDir = (await commands.gitRepoInfo(dir).catch(() => null))?.root ?? dir;
  openDialog(CreateWorktreeDialog, {
    codexHome: codexHome(),
    repoDir,
    submit: async (path: string, branch: WorktreeBranchRequest) => {
      await gitWorktreeAdd(repoDir, path, branch);
      quietRefresh();
    },
  });
}

export async function menuAction(action: MenuAction, target: MenuTarget): Promise<void> {
  try {
    if (
      target.kind === "project" &&
      target.project.kind === "multiProject" &&
      !["reveal", "openDetails", "newFolder"].includes(action)
    ) {
      return;
    }
    // Each action names the target kinds it applies to; on any other target it
    // does nothing.
    switch (action) {
      case "rename":
        await rename(target);
        return;
      case "newFolder":
        if (target.kind === "project") await newFolder(target.project.path, null);
        else if (target.kind === "folder") await newFolder(target.folder.scope, target.folder.id);
        return;
      case "deleteFolder":
        if (target.kind === "folder") await applyFresh(commands.deleteSidebarFolder(target.folder.id));
        return;
      case "deleteSection":
        if (target.kind === "section") await applyFresh(commands.deleteThreadSection(target.section.id));
        return;
      case "openDetails":
        if (target.kind === "project") openProjectDetail(target.project);
        return;
      case "openGit":
        if (target.kind === "project") openProjectDetail(target.project, "git");
        return;
      case "openWorktrees":
        if (target.kind === "project") openProjectDetail(target.project, "worktrees");
        return;
      case "toggleArchive":
        if (target.kind !== "project") return;
        await applyFresh(commands.setProjectArchived(target.project.path, !target.project.archived));
        if (view.projectPath === target.project.path && !target.project.archived) goHome();
        return;
      case "remove":
        if (target.kind !== "project") return;
        await applyFresh(commands.removeProject(target.project.path));
        if (view.projectPath === target.project.path) goHome();
        return;
      case "unhideAll": {
        if (target.kind !== "project") return;
        const ids = target.project.threads.filter((thread) => thread.hidden).map((thread) => thread.id);
        if (ids.length > 0) await applyFresh(commands.setThreadsHidden(ids, false));
        return;
      }
      case "resetOrder":
        if (target.kind === "project") await resetOrder(target.project.path);
        return;
      case "moveUp":
      case "moveDown":
        if (target.kind === "project") await nudgeProject(target.project, action === "moveUp" ? -1 : 1);
        return;
      case "addWorktree":
        if (target.kind === "project") await addWorktreeFor(target.project);
        return;
      case "toggleHidden":
        if (target.kind === "thread")
          await applyFresh(commands.setThreadsHidden([target.thread.id], !target.thread.hidden));
        return;
      case "hideBelow": {
        if (target.kind !== "thread") return;
        const at = target.order.indexOf(target.thread.id);
        const below = new Set(at < 0 ? [] : target.order.slice(at + 1));
        const ids = target.project.threads
          .filter((thread) => below.has(thread.id) && !thread.hidden && !thread.pinned)
          .map((thread) => thread.id);
        if (ids.length > 0) await applyFresh(commands.setThreadsHidden(ids, true));
        return;
      }
      case "fork": {
        if (target.kind !== "thread") return;
        const forked = await forkThread(target.thread.id);
        setView({ projectPath: target.project.path, threadId: forked.id });
        quietRefresh();
        return;
      }
      case "createWorktree":
        if (target.kind === "thread") await createWorktreeFor(target.thread.cwd || target.project.path);
        return;
      case "moveToWorkspace":
        if (target.kind === "thread") await moveToWorkspace(target.thread.id);
        return;
      case "moveToSection":
        if (target.kind === "thread") await moveToSection(target.thread.id, target.thread.sectionId);
        return;
      case "archive": {
        if (target.kind !== "thread") return;
        const ticket = dataTicket();
        const archived = await commands.archiveThread(target.thread.id);
        forgetOptimisticThread(target.thread.id);
        applyTicketed(archived, ticket);
        clearThread(target.thread.id);
        return;
      }
      case "reveal":
        if (target.kind === "project") await commands.revealInFinder(target.project.path);
        else if (target.kind === "thread") await commands.revealInFinder(target.thread.cwd || target.project.path);
        return;
      case "delete":
        if (target.kind === "project" || target.kind === "thread") await confirmDelete(target);
        return;
      case "togglePin":
        if (target.kind === "project") {
          await applyFresh(commands.setProjectPinned(target.project.path, !target.project.pinned));
        } else if (target.kind === "thread") {
          await applyFresh(commands.setThreadPinned(target.thread.id, !target.thread.pinned));
        }
        return;
      default:
        action satisfies never;
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
      const next = await applyFresh(
        workspace?.workspaceId ? updateWorkspace(workspace.workspaceId, input) : commands.createWorkspace(input),
      );
      const saved = workspace?.workspaceId
        ? next.projects.find((project) => project.workspaceId === workspace.workspaceId)
        : next.projects.find((project) => project.kind === "multiProject" && project.name === input.name);
      if (saved) openProjectDetail(saved);
    },
  });
}
