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
import { onMount } from "svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  gitRecentCommits,
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
import type { GitCommit, GitRepoInfo, Project, WorktreeBranchRequest, WorktreeEntry } from "$lib/types";
import CreateWorktreeDialog from "$lib/worktrees/CreateWorktreeDialog.svelte";
import RemoveWorktreeDialog from "$lib/worktrees/RemoveWorktreeDialog.svelte";
import { isTempWorktreePath, type WorktreeCard, worktreeCards } from "$lib/worktrees/worktrees";

let {
  repoDir,
  repoName,
  projects = [],
  codexHome = null,
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
  onBack: () => void;
  onOpenInApp: (path: string) => void;
  onRevealInFinder: (path: string) => void;
  onNewThread: (cwd: string) => void;
  onReview?: () => void;
  onRenameProject?: (path: string) => void;
} = $props();

let entries = $state<WorktreeEntry[]>([]);
let repoInfo = $state<GitRepoInfo | null>(null);
let commits = $state<GitCommit[]>([]);
let loading = $state(true);
let error = $state<string | null>(null);
let actionError = $state<string | null>(null);

let openMenu = $state<string | null>(null);

const cards = $derived<WorktreeCard[]>(worktreeCards(entries, projects));
const mainCard = $derived(cards.find((card) => card.entry.isMain) ?? null);
const linkedCards = $derived(cards.filter((card) => !card.entry.isMain));
const hasStale = $derived(entries.some((entry) => entry.prunable || entry.missingDir));

async function load() {
  loading = true;
  error = null;
  try {
    const [worktrees, info, recent] = await Promise.all([
      gitWorktrees(repoDir),
      gitRepoInfo(repoDir),
      gitRecentCommits(repoDir, 20).catch(() => []),
    ]);
    entries = worktrees;
    repoInfo = info;
    commits = recent;
  } catch (cause) {
    error = cause instanceof Error ? cause.message : String(cause);
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
  actionError = null;
  openMenu = null;
  try {
    await fn();
    await load();
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  }
}

function newWorktree() {
  openDialog(CreateWorktreeDialog, {
    codexHome,
    repoDir,
    commits,
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
  openMenu = null;
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

async function removeWorktree(entry: WorktreeEntry) {
  openMenu = null;
  const result = await openDialog(RemoveWorktreeDialog, {
    entry,
    threadCount: threadCountFor(entry.path),
  });
  if (result) await runAction(() => gitWorktreeRemove(repoDir, entry.path, result.force));
}
</script>

<div class="h-full overflow-y-auto">
  <div class="mx-auto max-w-3xl px-6 py-6">
    <div class="flex items-center gap-2">
      <TooltipButton label="Back" onclick={onBack} aria-label="Back" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
        <ArrowLeft size={16} />
      </TooltipButton>
      <div class="min-w-0 flex-1">
        <h1 class="flex items-center gap-2 text-lg font-semibold tracking-[-0.02em]">
          <FolderGit2 size={18} class="text-primary-500" />
          <span class="truncate">{repoName}</span>
        </h1>
        <p class="truncate text-[11px] text-surface-500" title={repoDir}>{repoDir}</p>
      </div>
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
      {#if actionError}
        <div class="mt-4 card preset-tonal-error p-3 text-xs">{actionError}</div>
      {/if}
      {#if repoInfo?.inProgress}
        <div class="mt-4 rounded-lg border border-warning-500/40 bg-warning-500/10 px-3 py-2 text-xs text-warning-700 dark:text-warning-300">
          A <span class="font-semibold">{repoInfo.inProgress}</span> is in progress in this repository.
        </div>
      {/if}

      {#if mainCard}
        <h2 class="mt-6 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Main checkout</h2>
        <div class="mt-2 rounded-xl border-2 border-primary-500/40 bg-surface-100-900 p-4">
          {@render cardBody(mainCard)}
        </div>
      {/if}

      <h2 class="mt-6 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">
        Linked worktrees
        <span class="ml-1 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal">{linkedCards.length}</span>
      </h2>
      {#if loading && entries.length === 0}
        <div class="mt-2 space-y-2">
          <div class="placeholder h-20 animate-pulse rounded-xl"></div>
          <div class="placeholder h-20 animate-pulse rounded-xl opacity-70"></div>
        </div>
      {:else if linkedCards.length === 0}
        <p class="mt-2 rounded-xl border border-dashed border-surface-300-700 px-4 py-6 text-center text-xs text-surface-500">
          No linked worktrees yet. Create one to work on a branch in an isolated checkout.
        </p>
      {:else}
        <div class="mt-2 space-y-2">
          {#each linkedCards as card (card.entry.path)}
            <div class="rounded-xl border border-surface-200-800 bg-surface-100-900 p-4">
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
      <div class="relative">
        <button
          type="button"
          onclick={() => (openMenu = openMenu === card.entry.path ? null : card.entry.path)}
          aria-label="Worktree actions"
          class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
        >
          <MoreHorizontal size={16} />
        </button>
        {#if openMenu === card.entry.path}
          <button type="button" class="fixed inset-0 z-40 cursor-default" aria-label="Close menu" onclick={() => (openMenu = null)}></button>
          <div class="absolute right-0 z-50 mt-1 w-48 overflow-hidden rounded-lg border border-surface-200-800 bg-surface-50-950 py-1 text-sm shadow-xl">
            {#if onRenameProject}
              <button onclick={() => { openMenu = null; onRenameProject?.(card.entry.path); }} class="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:preset-tonal">
                <Pencil size={13} /> Rename display name
              </button>
            {/if}
            {#if card.entry.locked}
              <button onclick={() => runAction(() => gitWorktreeUnlock(repoDir, card.entry.path))} class="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:preset-tonal">
                <LockOpen size={13} /> Unlock
              </button>
            {:else if !card.entry.isMain}
              <button onclick={() => runAction(() => gitWorktreeLock(repoDir, card.entry.path))} class="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:preset-tonal">
                <Unlock size={13} /> Lock
              </button>
            {/if}
            {#if card.entry.prunable || card.entry.missingDir}
              <button onclick={() => runAction(() => gitWorktreePrune(repoDir))} class="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:preset-tonal">
                <Trash2 size={13} /> Prune stale
              </button>
            {/if}
            {#if !card.entry.isMain && isTempWorktreePath(card.entry.path) && !card.entry.missingDir}
              <button onclick={() => handoffToLocal(card.entry)} class="flex w-full items-center gap-2 px-3 py-1.5 text-left hover:preset-tonal">
                <ArrowLeftRight size={13} /> Hand off to local
              </button>
            {/if}
            {#if !card.entry.isMain}
              <button onclick={() => removeWorktree(card.entry)} class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-error-500 hover:preset-tonal">
                <Trash2 size={13} /> Remove worktree
              </button>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/snippet}
