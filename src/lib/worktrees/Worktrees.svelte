<script lang="ts">
import {
  AlertTriangle,
  ArrowLeft,
  ArrowLeftRight,
  Bot,
  FolderGit2,
  FolderOpen,
  GitBranch,
  GitPullRequest,
  Lock,
  LockOpen,
  MoreHorizontal,
  Pencil,
  Plus,
  RefreshCw,
  SquarePen,
  Trash2,
  Unlock,
} from "@lucide/svelte";
import { Menu, Portal } from "@skeletonlabs/skeleton-svelte";
import { onMount } from "svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  gitRepoInfo,
  gitWorktreeAdd,
  gitWorktreeHandoff,
  gitWorktreeLock,
  gitWorktreePrune,
  gitWorktreeRemove,
  gitWorktrees,
  gitWorktreeUnlock,
} from "$lib/services/api";
import { type CodexEvent, setThreadHandler } from "$lib/services/codexEvents.svelte";
import HandoffToLocalDialog from "$lib/thread/HandoffToLocalDialog.svelte";
import type { GitRepoInfo, Project, WorktreeBranchRequest, WorktreeEntry } from "$lib/types";
import { submitState } from "$lib/utils/actionState.svelte";
import { errorMessage } from "$lib/utils/errors";
import CreateWorktreeDialog from "$lib/worktrees/CreateWorktreeDialog.svelte";
import { refreshGitStatus } from "$lib/worktrees/gitStatus.svelte";
import RemoveWorktreeDialog from "$lib/worktrees/RemoveWorktreeDialog.svelte";
import { isTempWorktreePath, type WorktreeCard, worktreeCards } from "$lib/worktrees/worktrees";

let {
  repoDir,
  repoName,
  projects = [],
  codexHome = null,
  embedded = false,
  onBack,
  onOpenInApp,
  onRevealInFinder,
  onNewThread,
  onReview,
  onRenameProject,
}: {
  repoDir: string;
  repoName: string;
  projects?: Project[];
  codexHome?: string | null;
  /** Rendered inside the project details tab: no back button or title. */
  embedded?: boolean;
  onBack?: () => void;
  onOpenInApp: (path: string) => void;
  onRevealInFinder: (path: string) => void;
  onNewThread: (cwd: string) => void;
  onReview?: () => void;
  onRenameProject?: (path: string) => void;
} = $props();

let entries = $state<WorktreeEntry[]>([]);
let repoInfo = $state<GitRepoInfo | null>(null);
let loading = $state(true);
let error = $state<string | null>(null);
/** Menu actions; they may overlap and share one error banner. */
const actions = submitState({ exclusive: false });

const cards = $derived<WorktreeCard[]>(worktreeCards(entries, projects));
const mainCard = $derived(cards.find((card) => card.entry.isMain) ?? null);
const linkedCards = $derived(cards.filter((card) => !card.entry.isMain));
const hasStale = $derived(entries.some((entry) => entry.prunable || entry.missingDir));

async function load() {
  loading = true;
  error = null;
  try {
    const [worktrees, info] = await Promise.all([gitWorktrees(repoDir), gitRepoInfo(repoDir)]);
    entries = worktrees;
    repoInfo = info;
  } catch (cause) {
    error = errorMessage(cause);
  } finally {
    loading = false;
  }
}

onMount(() => {
  load();
  // Refresh after a completed agent turn (bounded — no continuous polling).
  return setThreadHandler((event: CodexEvent) => {
    if (event.method === "turn/completed") load();
  });
});

async function runAction(fn: () => Promise<void>) {
  await actions.run(async () => {
    await fn();
    await load();
    void refreshGitStatus(repoDir);
  });
}

function newWorktree() {
  openDialog(CreateWorktreeDialog, {
    codexHome,
    repoDir,
    submit: async (path: string, branch: WorktreeBranchRequest) => {
      await gitWorktreeAdd(repoDir, path, branch);
      await load();
      // Creating a worktree should produce a thread whose cwd is that worktree.
      onNewThread(path);
    },
  });
}

function threadCountFor(path: string): number {
  return cards.find((card) => card.entry.path === path)?.threadCount ?? 0;
}

/** Check a temporary worktree's branch out in this repository and drop the worktree. */
function handoffToLocal(entry: WorktreeEntry) {
  openDialog(HandoffToLocalDialog, {
    worktreePath: entry.path,
    targets: [],
    defaultTarget: repoDir,
    submit: async (targetDir: string, commitUncommitted: boolean, branchName: string | null) => {
      await gitWorktreeHandoff(entry.path, targetDir, commitUncommitted, branchName);
      await load();
    },
  });
}

/** The actions a worktree's overflow menu offers, in order. */
function menuItems(entry: WorktreeEntry) {
  const items: { value: string; label: string; icon: typeof Pencil; danger?: boolean }[] = [];
  if (onRenameProject) items.push({ value: "rename", label: "Rename display name", icon: Pencil });
  if (entry.locked) items.push({ value: "unlock", label: "Unlock", icon: LockOpen });
  else if (!entry.isMain) items.push({ value: "lock", label: "Lock", icon: Unlock });
  if (entry.prunable || entry.missingDir) items.push({ value: "prune", label: "Prune stale", icon: Trash2 });
  if (!entry.isMain && isTempWorktreePath(entry.path) && !entry.missingDir)
    items.push({ value: "handoff", label: "Hand off to local", icon: ArrowLeftRight });
  if (!entry.isMain) items.push({ value: "remove", label: "Remove worktree", icon: Trash2, danger: true });
  return items;
}

function menuAction(entry: WorktreeEntry, action: string) {
  if (action === "rename") onRenameProject?.(entry.path);
  else if (action === "unlock") void runAction(() => gitWorktreeUnlock(repoDir, entry.path));
  else if (action === "lock") void runAction(() => gitWorktreeLock(repoDir, entry.path));
  else if (action === "prune") void runAction(() => gitWorktreePrune(repoDir));
  else if (action === "handoff") handoffToLocal(entry);
  else if (action === "remove") void removeWorktree(entry);
}

async function removeWorktree(entry: WorktreeEntry) {
  const result = await openDialog(RemoveWorktreeDialog, {
    entry,
    threadCount: threadCountFor(entry.path),
  });
  if (result) await runAction(() => gitWorktreeRemove(repoDir, entry.path, result.force));
}
</script>

<div class={embedded ? "" : "h-full overflow-y-auto"}>
  <div class={embedded ? "" : "mx-auto max-w-3xl px-6 py-6"}>
    <div class="flex items-center gap-2">
      {#if !embedded}
        <TooltipButton label="Back" onclick={() => onBack?.()} aria-label="Back" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
          <ArrowLeft size={16} />
        </TooltipButton>
        <div class="min-w-0 flex-1">
          <h1 class="flex items-center gap-2 text-lg font-semibold tracking-[-0.02em]">
            <FolderGit2 size={18} class="text-primary-500" />
            <span class="truncate">{repoName}</span>
          </h1>
          <p class="truncate text-[11px] text-surface-500" title={repoDir}>{repoDir}</p>
        </div>
      {:else}
        <div class="min-w-0 flex-1 text-xs text-surface-500">Worktrees of <span class="font-mono">{repoName}</span></div>
      {/if}
      {#if hasStale}
        <button onclick={() => runAction(() => gitWorktreePrune(repoDir))} class="btn btn-sm preset-tonal">
          <Trash2 size={14} />
          Prune stale
        </button>
      {/if}
      {#if onReview}
        <button onclick={onReview} class="btn btn-sm preset-tonal">
          <GitPullRequest size={14} />
          Review
        </button>
      {/if}
      <button onclick={newWorktree} class="btn btn-sm preset-filled-primary-500">
        <Plus size={14} />
        New worktree
      </button>
      <TooltipButton label="Refresh worktrees" onclick={load} aria-label="Refresh worktrees" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
        <RefreshCw size={15} class={loading ? "animate-spin" : ""} />
      </TooltipButton>
    </div>

    {#if repoInfo && !repoInfo.isGitRepo}
      <div class="mt-6 card preset-tonal-warning p-4 text-sm">
        <div class="flex items-center gap-1.5 font-semibold"><AlertTriangle size={15} /> Not a Git repository</div>
        <p class="mt-1 text-xs leading-5">{repoInfo.error ?? "This folder is not tracked by Git, so it has no worktrees."}</p>
      </div>
    {:else if error}
      <div class="mt-6 card preset-tonal-error p-4 text-sm">
        <div class="font-semibold">Could not read worktrees</div>
        <p class="mt-1 text-xs leading-5">{error}</p>
      </div>
    {:else}
      {#if actions.error}
        <div class="mt-4 error-note p-3">{actions.error}</div>
      {/if}
      {#if repoInfo?.inProgress}
        <div class="mt-4 rounded-lg border border-warning-500/40 bg-warning-500/10 px-3 py-2 text-xs text-warning-700 dark:text-warning-300">
          A <span class="font-semibold">{repoInfo.inProgress}</span> is in progress in this repository.
        </div>
      {/if}

      {#if mainCard}
        <h2 class="{embedded ? 'mt-4' : 'mt-6'} eyebrow">Main checkout</h2>
        <div class="mt-2 rounded-xl border-2 border-primary-500/40 bg-surface-100-900 p-4">
          {@render cardBody(mainCard)}
        </div>
      {/if}

      <h2 class="mt-6 eyebrow">
        Linked worktrees
        <span class="ml-1 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal">{linkedCards.length}</span>
      </h2>
      {#if loading && entries.length === 0}
        <div class="mt-2 space-y-2">
          <div class="placeholder h-20 animate-pulse rounded-xl"></div>
          <div class="placeholder h-20 animate-pulse rounded-xl opacity-70"></div>
        </div>
      {:else if linkedCards.length === 0}
        <p class="mt-2 empty-state">
          No linked worktrees yet. Create one to work on a branch in an isolated checkout.
        </p>
      {:else}
        <div class="mt-2 space-y-2">
          {#each linkedCards as card (card.entry.path)}
            <div class="panel-card p-4">
              {@render cardBody(card)}
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>
</div>

{#snippet cardBody(card: WorktreeCard)}
  <div class="flex items-start gap-3">
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2">
        <span class="truncate text-sm font-semibold">{card.displayName}</span>
        {#if card.entry.isMain}
          <span class="rounded-full bg-primary-500/15 px-1.5 py-0.5 text-[10px] font-medium text-primary-500">main</span>
        {/if}
        {#if card.entry.isCodexManaged}
          <span class="rounded-full bg-primary-500/15 px-1.5 py-0.5 text-[10px] font-medium text-primary-500" title="Managed under CODEX_HOME/worktrees">
            Codex-managed
          </span>
        {/if}
        {#if card.entry.locked}
          <span class="inline-flex items-center gap-0.5 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium text-surface-500" title={card.entry.lockReason ?? "Locked"}>
            <Lock size={9} /> locked
          </span>
        {/if}
      </div>
      <div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-surface-500">
        <span class="inline-flex items-center gap-1"><GitBranch size={11} />{card.branchLabel}</span>
        <span class={card.dirty ? "text-warning-600 dark:text-warning-400" : ""}>{card.statusLabel}</span>
        {#if card.aheadBehind}<span class="font-mono">{card.aheadBehind}</span>{/if}
        {#if card.threadCount > 0}
          <span class="inline-flex items-center gap-1"><Bot size={11} />{card.threadCount} thread{card.threadCount === 1 ? "" : "s"}</span>
        {/if}
      </div>
      <div class="mt-1 truncate font-mono text-[10px] text-surface-500" title={card.entry.path}>{card.entry.path}</div>
      {#if card.problem}
        <div class="mt-2 flex items-start gap-1.5 rounded-md bg-warning-500/10 px-2 py-1 text-[11px] leading-4 text-warning-700 dark:text-warning-300">
          <AlertTriangle size={12} class="mt-px shrink-0" />
          <span>{card.problem}</span>
        </div>
      {/if}
    </div>

    <div class="flex shrink-0 items-center gap-1">
      {#if !card.entry.missingDir}
        <button onclick={() => onOpenInApp(card.entry.path)} class="btn btn-sm preset-tonal" title="Open in app">Open</button>
        <TooltipButton label="New thread" onclick={() => onNewThread(card.entry.path)} aria-label="New thread here" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
          <SquarePen size={14} />
        </TooltipButton>
        <TooltipButton label="Reveal in Finder" onclick={() => onRevealInFinder(card.entry.path)} aria-label="Reveal in Finder" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
          <FolderOpen size={14} />
        </TooltipButton>
      {/if}
      <Menu positioning={{ placement: "bottom-end" }} onSelect={(details) => menuAction(card.entry, details.value)}>
        <Menu.Trigger aria-label="Worktree actions" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
          <MoreHorizontal size={16} />
        </Menu.Trigger>
        <Portal>
          <Menu.Positioner>
            <Menu.Content class="popover-card z-50 w-48 select-none p-1 text-sm">
              {#each menuItems(card.entry) as item (item.value)}
                <Menu.Item
                  value={item.value}
                  class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 hover:preset-tonal data-highlighted:preset-tonal {item.danger ? 'text-error-500' : ''}"
                >
                  <item.icon size={13} />
                  <Menu.ItemText>{item.label}</Menu.ItemText>
                </Menu.Item>
              {/each}
            </Menu.Content>
          </Menu.Positioner>
        </Portal>
      </Menu>
    </div>
  </div>
{/snippet}
