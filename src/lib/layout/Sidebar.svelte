<script lang="ts">
import {
  Bot,
  ChevronDown,
  Ellipsis,
  Folder,
  FolderGit2,
  GitBranch,
  Layers3,
  MessageCircleQuestion,
  Pin,
  Plus,
  Search,
  Settings,
  Smartphone,
  SquarePen,
  Star,
} from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import ArchivedThreadsSection from "$lib/layout/ArchivedThreadsSection.svelte";
import { activeConnectionCount, hasActiveConnection } from "$lib/layout/connectionState";
import SidebarContextMenu from "$lib/layout/SidebarContextMenu.svelte";
import SidebarSearch from "$lib/layout/SidebarSearch.svelte";
import UsageMeter from "$lib/layout/UsageMeter.svelte";
import { accountUsage } from "$lib/services/accountUsage.svelte";
import { activeTurns, approvals, unansweredQuestions, userInputRequests } from "$lib/services/codexEvents.svelte";
import { remoteConnections } from "$lib/services/connections.svelte";
import type {
  Account,
  ArchivedThread,
  BootstrapData,
  MenuAction,
  MenuTarget,
  Project,
  SideQuestion,
  ThreadSearchItem,
  ThreadSummary,
} from "$lib/types";
import { dragRegion } from "$lib/utils/dragRegion";
import { relativeTime } from "$lib/utils/time";
import BranchChip from "$lib/worktrees/BranchChip.svelte";
import { ensureGitStatus, gitStatusCache } from "$lib/worktrees/gitStatus.svelte";
import { folderName, isTempWorktreePath } from "$lib/worktrees/worktrees";

let {
  projects,
  account,
  selectedThread,
  loading,
  sideQuestions = [],
  onAddProject,
  onAddWorkspace,
  onSelectThread,
  onNewThread,
  onGoHome,
  onOpenSettings,
  onMenuAction,
  onSelectArchived,
  onUnarchived,
  onOpenWorktrees,
  currentProject = null,
  onOpenSearchResult,
}: {
  projects: Project[];
  account: Account | null;
  selectedThread: string | null;
  loading: boolean;
  sideQuestions?: SideQuestion[];
  onAddProject: () => void;
  onAddWorkspace?: () => void;
  onSelectThread: (project: Project, threadId: string) => void;
  onNewThread: (project?: Project) => void;
  onGoHome?: () => void;
  onOpenSettings: () => void;
  onMenuAction: (action: MenuAction, target: MenuTarget) => void;
  onSelectArchived?: (thread: ArchivedThread) => void;
  onUnarchived?: (data: BootstrapData) => void;
  onOpenWorktrees?: (project: Project) => void;
  currentProject?: Project | null;
  onOpenSearchResult?: (item: ThreadSearchItem) => void;
} = $props();

const visibleProjects = $derived(projects.filter((project) => !project.archived));

// One busy project shouldn't push every other project off-screen, so expanded
// projects show a head slice until the user asks for the rest.
const THREAD_LIMIT = 15;
let showAllThreads = $state<Record<string, boolean>>({});

function visibleThreads(project: Project) {
  if (showAllThreads[project.path] || project.threads.length <= THREAD_LIMIT) return project.threads;
  const head = project.threads.slice(0, THREAD_LIMIT);
  // Selection can come from outside the sidebar; keep it visible even when it
  // sorts past the cap.
  if (selectedThread && !head.some((thread) => thread.id === selectedThread)) {
    const selected = project.threads.find((thread) => thread.id === selectedThread);
    if (selected) head.push(selected);
  }
  return head;
}

let searching = $state(false);

function openSearch() {
  searching = true;
}

function onWindowKeydown(event: KeyboardEvent) {
  // Cmd/Ctrl+Shift+F opens sidebar search (Shift avoids clashing with the
  // in-thread find shortcut).
  if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "f") {
    event.preventDefault();
    searching = true;
  }
}

// "waiting" wins over "working": a blocked turn still counts as inProgress,
// but the user action needed is the more useful signal. Questions stranded by
// an earlier session count too — the sidebar is the only way to find them.
const threadActivity = (threadId: string): "waiting" | "working" | null => {
  if (
    approvals.list.some((approval) => approval.threadId === threadId) ||
    userInputRequests.list.some((request) => request.threadId === threadId) ||
    unansweredQuestions.list.includes(threadId)
  ) {
    return "waiting";
  }
  return activeTurns.list.includes(threadId) ? "working" : null;
};

const sideQuestionCount = (threadId: string) =>
  sideQuestions.filter((entry) => entry.parentThreadId === threadId).length;

let menu = $state<{ x: number; y: number; target: MenuTarget } | null>(null);

const MENU_WIDTH = 190;

function openMenuAt(x: number, y: number, target: MenuTarget) {
  const height = target.kind === "thread" ? 230 : 230;
  menu = {
    x: Math.min(x, window.innerWidth - MENU_WIDTH - 8),
    y: Math.min(y, window.innerHeight - height - 8),
    target,
  };
}

function onContextMenu(event: MouseEvent, target: MenuTarget) {
  event.preventDefault();
  event.stopPropagation();
  openMenuAt(event.clientX, event.clientY, target);
}

function onEllipsis(event: MouseEvent, target: MenuTarget) {
  event.preventDefault();
  event.stopPropagation();
  const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
  openMenuAt(rect.left, rect.bottom + 4, target);
}

function act(action: MenuAction) {
  if (!menu) return;
  const target = menu.target;
  menu = null;
  onMenuAction(action, target);
}

const connectedDevices = $derived(hasActiveConnection(remoteConnections.list));
const onlineCount = $derived(activeConnectionCount(remoteConnections.list));

const projectTarget = (project: Project): MenuTarget => ({ kind: "project", project });

/** Tooltip body for a project row: full path plus the checked-out branch once known. */
function projectTooltip(project: Project): string {
  const branch = gitStatusCache.byPath[project.path]?.branch;
  return branch ? `${project.path}\n⎇ ${branch}` : project.path;
}
const threadTarget = (project: Project, thread: ThreadSummary): MenuTarget => ({
  kind: "thread",
  project,
  thread,
});
</script>

<svelte:window onkeydown={onWindowKeydown} />

<aside class="flex h-full w-full flex-col border-r border-surface-200-800 bg-surface-100-900 text-surface-900-100">
  <div class="h-7 shrink-0 select-none" data-tauri-drag-region use:dragRegion></div>
  <div class="flex h-12 items-center justify-between px-3 select-none" data-tauri-drag-region use:dragRegion>
    <button
      onclick={() => onGoHome?.()}
      class="flex items-center gap-2.5 rounded-lg font-semibold tracking-[-0.02em] hover:opacity-80"
      title="Home"
    >
      <div class="grid size-7 place-items-center rounded-lg preset-filled-primary-500 text-[11px] font-bold">P</div>
      Pingex
    </button>
    <div class="flex items-center gap-0.5">
      <TooltipButton
        label="Search threads (⌘⇧F)"
        aria-label="Search threads"
        onclick={openSearch}
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-600-400"
      >
        <Search size={16} strokeWidth={1.8} />
      </TooltipButton>
      <TooltipButton
        label="New thread"
        onclick={() => onNewThread()}
        disabled={visibleProjects.length === 0}
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-600-400 disabled:opacity-40"
      >
        <SquarePen size={17} strokeWidth={1.8} />
      </TooltipButton>
    </div>
  </div>

  {#if searching}
    <SidebarSearch
      {projects}
      currentProjectPath={currentProject?.path ?? null}
      currentProjectName={currentProject?.name ?? null}
      onOpenResult={(item) => onOpenSearchResult?.(item)}
      onClose={() => (searching = false)}
    />
  {:else}

  <div class="group/projects min-h-0 flex-1 select-none overflow-y-auto px-2 pb-3 pt-3">
    <div class="projects-heading mb-1 flex h-8 items-center justify-between px-2">
      <span class="text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Projects</span>
      <div class="flex items-center gap-0.5">
        {#if onAddWorkspace}
          <TooltipButton
            label="Create multi-project workspace"
            onclick={onAddWorkspace}
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-600-400 opacity-0 transition focus:opacity-100 group-hover/projects:opacity-100"
          >
            <Layers3 size={15} />
          </TooltipButton>
        {/if}
        <TooltipButton
          label="Add project folder"
          onclick={onAddProject}
          class="btn-icon btn-icon-sm hover:preset-tonal text-surface-600-400 opacity-0 transition focus:opacity-100 group-hover/projects:opacity-100"
        >
          <Plus size={15} />
        </TooltipButton>
      </div>
    </div>

    {#if loading}
      <div class="space-y-2 px-2 py-2" aria-label="Loading projects">
        <div class="placeholder h-8 animate-pulse rounded-md"></div>
        <div class="placeholder h-8 animate-pulse rounded-md opacity-70"></div>
      </div>
    {:else if visibleProjects.length === 0}
      <button onclick={onAddProject} class="mx-2 mt-2 rounded-lg border border-dashed border-surface-300-700 px-3 py-4 text-left text-xs leading-5 text-surface-600-400 hover:border-surface-400-600 hover:text-surface-800-200">
        Add a folder to see its Codex threads here.
      </button>
    {:else}
      <div class="space-y-0.5">
        {#each visibleProjects as project (project.path)}
          <Collapsible defaultOpen={true}>
            <div
              class="group/project relative flex items-center"
              role="presentation"
              oncontextmenu={(event) => onContextMenu(event, projectTarget(project))}
            >
              <Collapsible.Trigger class="group flex min-w-0 flex-1 items-center gap-2 rounded-md py-1.5 pl-2 pr-14 text-left text-[13px] font-medium group-hover/project:preset-tonal">
                <ChevronDown class="text-surface-500 transition group-data-[state=closed]:-rotate-90" size={14} />
                {#if project.kind === "multiProject"}
                  <Layers3 class="text-primary-500" size={15} strokeWidth={1.7} />
                {:else if project.kind === "worktree"}
                  <TooltipAnchor label="Codex worktree" class="grid place-items-center">
                    <FolderGit2 class="text-primary-500" size={15} strokeWidth={1.7} />
                  </TooltipAnchor>
                {:else}
                  <Folder class="text-surface-500" size={15} strokeWidth={1.7} />
                {/if}
                <TooltipAnchor
                  label={projectTooltip(project)}
                  multiline={true}
                  role="presentation"
                  aria-label={projectTooltip(project)}
                  class="min-w-0 flex-1 truncate"
                  onmouseenter={() => ensureGitStatus(project.path)}
                >{project.name}</TooltipAnchor>
                {#if project.pinned}
                  <Pin class="shrink-0 text-surface-500" size={11} />
                {/if}
                <span class="text-[10px] font-normal text-surface-500 group-hover/project:opacity-0">{project.threads.length}</span>
              </Collapsible.Trigger>
              <TooltipButton
                label={`New thread in ${project.name}`}
                onclick={(event) => {
                  event.stopPropagation();
                  onNewThread(project);
                }}
                class="absolute right-7 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-300-700 hover:text-surface-900-100 focus:opacity-100 group-hover/project:opacity-100"
              >
                <SquarePen size={13} />
              </TooltipButton>
              <button
                type="button"
                aria-label="Project menu"
                onclick={(event) => onEllipsis(event, projectTarget(project))}
                class="absolute right-1 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-300-700 hover:text-surface-900-100 focus:opacity-100 group-hover/project:opacity-100"
              >
                <Ellipsis size={14} />
              </button>
              {#if onOpenWorktrees && project.kind !== "multiProject"}
                <div class="ml-1 shrink-0 group-hover/project:hidden">
                  <BranchChip path={project.path} onOpen={() => onOpenWorktrees?.(project)} maxWidthClass="max-w-[6rem]" />
                </div>
              {/if}
            </div>
            <Collapsible.Content class="ml-[27px] border-l border-surface-200-800 pl-1.5">
              {#if project.threads.length === 0}
                <p class="px-2 py-2 text-xs text-surface-500">No Codex threads yet</p>
              {:else}
                {#each visibleThreads(project) as thread (thread.id)}
                  <div
                    class="group/thread relative"
                    role="presentation"
                    oncontextmenu={(event) => onContextMenu(event, threadTarget(project, thread))}
                  >
                    <button
                      onclick={() => onSelectThread(project, thread.id)}
                      class="flex w-full items-center gap-2 rounded-md py-1.5 pl-2 pr-7 text-left text-[12px] transition {selectedThread === thread.id ? 'preset-tonal' : 'text-surface-700-300 hover:preset-tonal'}"
                    >
                      {#if threadActivity(thread.id) === "waiting"}
                        <span class="relative grid size-2 shrink-0 place-items-center" role="status" title="Waiting for your input">
                          <span class="absolute size-2 animate-ping rounded-full bg-warning-500/60"></span>
                          <span class="size-2 rounded-full bg-warning-500"></span>
                        </span>
                      {:else if threadActivity(thread.id) === "working"}
                        <span class="grid size-2 shrink-0 place-items-center" role="status" title="Working">
                          <span class="size-2 animate-pulse rounded-full bg-primary-500"></span>
                        </span>
                      {/if}
                      {#if thread.pinned}
                        <Star class="shrink-0 fill-warning-500 text-warning-500" size={11} />
                      {/if}
                      <span class="min-w-0 flex-1 truncate" title={thread.title}>{thread.title}</span>
                      {#if isTempWorktreePath(thread.cwd ?? "")}
                        <span
                          class="grid shrink-0 place-items-center text-surface-500"
                          title="Runs in the temporary worktree {folderName(thread.cwd ?? '')}"
                        >
                          <GitBranch size={10} />
                        </span>
                      {/if}
                      {#if sideQuestionCount(thread.id) > 0}
                        <span
                          class="inline-flex shrink-0 items-center gap-0.5 rounded-full bg-primary-500/15 px-1.5 text-[9px] font-medium text-primary-500"
                          title="{sideQuestionCount(thread.id)} side question{sideQuestionCount(thread.id) === 1 ? '' : 's'}"
                        >
                          <MessageCircleQuestion size={9} />
                          {sideQuestionCount(thread.id)}
                        </span>
                      {/if}
                      {#if (thread.subagentCount ?? 0) > 0}
                        <span
                          class="inline-flex shrink-0 items-center gap-0.5 rounded-full bg-success-500/15 px-1.5 text-[9px] font-medium text-success-600 dark:text-success-400"
                          title="{thread.subagentCount} subagent{thread.subagentCount === 1 ? '' : 's'}"
                        >
                          <Bot size={9} />
                          {thread.subagentCount}
                        </span>
                      {/if}
                      <span class="shrink-0 text-[10px] text-surface-500 group-hover/thread:opacity-0">{relativeTime(thread.updatedAt)}</span>
                    </button>
                    <button
                      type="button"
                      aria-label="Thread menu"
                      onclick={(event) => onEllipsis(event, threadTarget(project, thread))}
                      class="absolute right-0.5 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-300-700 hover:text-surface-900-100 focus:opacity-100 group-hover/thread:opacity-100"
                    >
                      <Ellipsis size={13} />
                    </button>
                  </div>
                {/each}
                {#if project.threads.length > THREAD_LIMIT}
                  <button
                    type="button"
                    onclick={() => (showAllThreads[project.path] = !showAllThreads[project.path])}
                    class="mt-0.5 flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-[11px] font-medium text-primary-500 hover:preset-tonal"
                  >
                    <ChevronDown class="transition {showAllThreads[project.path] ? '' : '-rotate-90'}" size={12} />
                    {showAllThreads[project.path]
                      ? "Show less"
                      : `Show ${project.threads.length - THREAD_LIMIT} more`}
                  </button>
                {/if}
              {/if}
            </Collapsible.Content>
          </Collapsible>
        {/each}
      </div>
    {/if}
  </div>

  <ArchivedThreadsSection {onSelectArchived} {onUnarchived} />
  {/if}

  <div class="border-t border-surface-200-800 p-2">
    {#if connectedDevices}
      <button
        onclick={onOpenSettings}
        title="Manage remote connections"
        class="mb-1 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-[11px] text-surface-600-400 transition hover:preset-tonal"
      >
        <span class="relative grid size-4 place-items-center">
          <Smartphone size={13} class="text-success-600 dark:text-success-400" />
          <span class="absolute -right-0.5 -top-0.5 size-1.5 rounded-full bg-success-500"></span>
        </span>
        <span class="min-w-0 flex-1 truncate">
          {#if onlineCount > 0}
            {onlineCount} device{onlineCount === 1 ? "" : "s"} online
          {:else}
            Device recently active
          {/if}
        </span>
      </button>
    {/if}
    <UsageMeter snapshot={accountUsage.snapshot} compact />
    <button onclick={onOpenSettings} class="group flex w-full items-center gap-2.5 rounded-lg p-2 text-left transition hover:preset-tonal">
      <div class="grid size-8 shrink-0 place-items-center rounded-full preset-filled-primary-500 text-xs font-semibold">
        {(account?.label ?? "?").slice(0, 1).toUpperCase()}
      </div>
      <div class="min-w-0 flex-1">
        <div class="truncate text-xs font-medium">{account?.label ?? "Not signed in"}</div>
        <div class="mt-0.5 text-[10px] capitalize text-surface-500">{account?.plan ?? account?.kind ?? "Codex account"}</div>
      </div>
      <Settings size={15} class="text-surface-500 transition group-hover:text-surface-800-200" />
    </button>
  </div>
</aside>

{#if menu}
  <SidebarContextMenu {menu} onAct={act} onClose={() => (menu = null)} />
{/if}
