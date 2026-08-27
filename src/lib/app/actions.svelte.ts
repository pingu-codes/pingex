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
import type { SlashCommandId } from "$lib/composer/slashCommands";
import DeleteThreadDialog from "$lib/layout/DeleteThreadDialog.svelte";
import RenameDialog from "$lib/layout/RenameDialog.svelte";
import SectionPickerDialog from "$lib/layout/SectionPickerDialog.svelte";
import {
  buildTree,
  childrenOf,
  emptyLayout,
  parentOf,
  placeInLayout,
  ROOT_SCOPE,
  refOf,
} from "$lib/layout/sidebarTree";
import {
  archiveThread,
  createSidebarFolder,
  createThreadSection,
  createWorkspace,
  deleteSidebarFolder,
  deleteThread,
  deleteThreadSection,
  forkThread,
  gitRepoInfo,
  gitWorktreeAdd,
  moveThreadToSection,
  moveThreadToWorkspace,
  placeSidebarItem,
  removeProject,
  renameProject,
  renameSidebarFolder,
  renameThread,
  revealInFinder,
  saveDraft,
  setProjectArchived,
  setProjectPinned,
  setThreadPinned,
  updateThreadSection,
  updateWorkspace,
} from "$lib/services/api";
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
import CreateWorktreeDialog from "$lib/worktrees/CreateWorktreeDialog.svelte";

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
    applyData(
      target.kind === "project"
        ? await renameProject(target.project.path, name)
        : target.kind === "thread"
          ? await renameThread(target.thread.id, name)
          : target.kind === "folder"
            ? await renameSidebarFolder(target.folder.id, name)
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
    applyData(await createSidebarFolder(scope, parentId, name));
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
    applyData(await placeSidebarItem(scope, item, parentId, siblings));
  } catch (cause) {
    data.sidebarLayout = previousLayout;
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

/** Create a permanent worktree for the repository containing `dir`. */
async function createWorktreeFor(dir: string): Promise<void> {
  const repoDir = (await gitRepoInfo(dir).catch(() => null))?.root ?? dir;
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
    if (action === "newFolder") {
      if (target.kind === "project") await newFolder(target.project.path, null);
      else if (target.kind === "folder") await newFolder(target.folder.scope, target.folder.id);
      return;
    }
    if (action === "deleteFolder") {
      if (target.kind === "folder") applyData(await deleteSidebarFolder(target.folder.id));
      return;
    }
    if (target.kind === "folder") {
      if (action === "rename") await rename(target);
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
      if (target.kind === "project") await nudgeProject(target.project, action === "moveUp" ? -1 : 1);
      return;
    }
    if (action === "fork") {
      if (target.kind !== "thread") return;
      const forked = await forkThread(target.thread.id);
      setView({ projectPath: target.project.path, threadId: forked.id });
      quietRefresh();
      return;
    }
    if (action === "createWorktree") {
      if (target.kind === "thread") await createWorktreeFor(target.thread.cwd || target.project.path);
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
