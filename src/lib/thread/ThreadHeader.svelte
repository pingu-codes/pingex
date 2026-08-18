<script lang="ts">
import { ArrowLeftRight, ChevronDown, Copy, GitBranch, Home, Link2, Send, TerminalSquare } from "@lucide/svelte";
import { projects } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import {
  forkThread,
  gitWorktreeHandoff,
  handoffCommand,
  handoffCopy,
  handoffLaunchTerminal,
  handoffThreadLink,
} from "$lib/services/api";
import HandoffConfirmDialog from "$lib/thread/HandoffConfirmDialog.svelte";
import HandoffToLocalDialog from "$lib/thread/HandoffToLocalDialog.svelte";
import { canHandoff, dirName, shortHomeName } from "$lib/thread/handoff";
import MoveToWorktreeDialog from "$lib/thread/MoveToWorktreeDialog.svelte";
import { ensureGitStatus, gitStatusCache, refreshGitStatus } from "$lib/worktrees/gitStatus.svelte";
import { isTempWorktreePath } from "$lib/worktrees/worktrees";

let {
  codexHome,
  repoName,
  repoDir,
  cwd,
  threadId,
  onMovedToWorktree,
  onError,
}: {
  codexHome: string | null;
  repoName: string | null;
  /** Repository root used to enumerate worktrees for "Move to worktree". */
  repoDir: string;
  /** The thread's working directory. */
  cwd: string;
  /** Live thread id; null for an unsaved draft (Handoff is disabled). */
  threadId: string | null;
  onMovedToWorktree: (forkedThreadId: string) => void;
  onError: (message: string) => void;
} = $props();

let menuOpen = $state(false);
let linkCopied = $state(false);

$effect(() => {
  if (cwd) ensureGitStatus(cwd);
});

const status = $derived(cwd ? (gitStatusCache.byPath[cwd] ?? null) : null);
const branch = $derived(status?.detached ? "detached" : (status?.branch ?? dirName(cwd)));
const home = $derived(shortHomeName(codexHome));
const enabled = $derived(canHandoff(threadId, cwd, codexHome));
const disabledHint = $derived(
  !threadId
    ? "Send a message to start the thread before handing it off"
    : !codexHome
      ? "Codex home is unavailable"
      : "",
);

function closeMenu() {
  menuOpen = false;
}

async function openContinueInTerminal() {
  closeMenu();
  if (!threadId) return;
  try {
    const command = await handoffCommand(threadId, cwd);
    openDialog(HandoffConfirmDialog, {
      home,
      threadId,
      dir: cwd,
      command,
      copy: handoffCopy,
      launch: handoffLaunchTerminal,
    });
  } catch (cause) {
    onError(cause instanceof Error ? cause.message : String(cause));
  }
}

async function copyThreadLink() {
  closeMenu();
  if (!threadId) return;
  try {
    const link = await handoffThreadLink(threadId, cwd);
    await handoffCopy(link);
    linkCopied = true;
    setTimeout(() => (linkCopied = false), 1500);
  } catch (cause) {
    onError(cause instanceof Error ? cause.message : String(cause));
  }
}

const inTempWorktree = $derived(isTempWorktreePath(cwd));

/** Check the temporary worktree's branch out locally and continue the thread there. */
function openHandoffToLocal() {
  closeMenu();
  const thread = threadId;
  if (!thread) return;
  const targets = projects()
    .filter((project) => project.kind === "folder" && !project.archived && !isTempWorktreePath(project.path))
    .map((project) => ({ path: project.path, name: project.name }));
  openDialog(HandoffToLocalDialog, {
    worktreePath: cwd,
    targets,
    defaultTarget: repoDir,
    submit: async (targetDir: string, commitUncommitted: boolean, branchName: string | null) => {
      await gitWorktreeHandoff(cwd, targetDir, commitUncommitted, branchName);
      refreshGitStatus(targetDir);
      const forked = await forkThread(thread, undefined, undefined, targetDir);
      onMovedToWorktree(forked.id);
    },
  });
}

function openMoveToWorktree() {
  closeMenu();
  const thread = threadId;
  if (!thread) return;
  openDialog(MoveToWorktreeDialog, {
    repoDir,
    currentCwd: cwd,
    submit: async (path: string) => {
      const forked = await forkThread(thread, undefined, undefined, path);
      onMovedToWorktree(forked.id);
    },
  });
}
</script>

<svelte:window onclick={closeMenu} />

<div class="flex items-center gap-2">
  <!-- home › repository › branch/worktree -->
  <nav class="hidden min-w-0 items-center gap-1 text-[11px] text-surface-500 sm:flex" aria-label="Thread location">
    <Home size={11} class="shrink-0" />
    <span class="max-w-[7rem] truncate" title={codexHome ?? undefined}>{home}</span>
    {#if repoName}
      <span class="text-surface-400-600">›</span>
      <span class="max-w-[9rem] truncate text-surface-600-400" title={repoDir}>{repoName}</span>
    {/if}
    {#if branch}
      <span class="text-surface-400-600">›</span>
      <span class="inline-flex min-w-0 items-center gap-1 text-surface-600-400">
        <GitBranch size={10} class="shrink-0" />
        <span class="max-w-[8rem] truncate" title={cwd}>{branch}</span>
      </span>
    {/if}
  </nav>

  <div class="relative">
    <button
      type="button"
      disabled={!enabled}
      title={enabled ? "Handoff options" : disabledHint}
      onclick={(event) => {
        event.stopPropagation();
        if (enabled) menuOpen = !menuOpen;
      }}
      class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-medium text-surface-600-400 transition hover:bg-surface-200-800 hover:text-surface-800-200 disabled:cursor-not-allowed disabled:opacity-40"
    >
      <Send size={12} />
      Handoff
      <ChevronDown size={11} />
    </button>
    {#if menuOpen && enabled}
      <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
      <div
        class="card absolute right-0 top-9 z-50 w-60 select-none border border-surface-200-800 bg-surface-50-950 p-1.5 shadow-xl"
        onclick={(event) => event.stopPropagation()}
        role="menu"
        aria-label="Handoff options"
      >
        <button onclick={openContinueInTerminal} class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal" role="menuitem">
          <TerminalSquare size={14} class="shrink-0 text-surface-500" />
          <span class="min-w-0 flex-1">Continue in terminal</span>
        </button>
        {#if inTempWorktree}
          <button onclick={openHandoffToLocal} class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal" role="menuitem">
            <ArrowLeftRight size={14} class="shrink-0 text-surface-500" />
            <span class="min-w-0 flex-1">Hand off to local</span>
          </button>
        {/if}
        <button onclick={openMoveToWorktree} class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal" role="menuitem">
          <GitBranch size={14} class="shrink-0 text-surface-500" />
          <span class="min-w-0 flex-1">Move to worktree</span>
        </button>
        <button onclick={copyThreadLink} class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal" role="menuitem">
          {#if linkCopied}<Copy size={14} class="shrink-0 text-primary-500" />{:else}<Link2 size={14} class="shrink-0 text-surface-500" />{/if}
          <span class="min-w-0 flex-1">{linkCopied ? "Link copied" : "Copy thread link"}</span>
        </button>
      </div>
    {/if}
  </div>
</div>
