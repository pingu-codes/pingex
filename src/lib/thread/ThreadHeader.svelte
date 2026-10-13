<script lang="ts">
import { ArrowLeftRight, ChevronDown, Copy, GitBranch, Home, Link2, Send, TerminalSquare } from "@lucide/svelte";
import { Menu, Portal } from "@skeletonlabs/skeleton-svelte";
import { projects } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import { forkThread } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import HandoffConfirmDialog from "$lib/thread/HandoffConfirmDialog.svelte";
import HandoffToLocalDialog from "$lib/thread/HandoffToLocalDialog.svelte";
import { canHandoff, dirName, shortHomeName } from "$lib/thread/handoff";
import MoveToWorktreeDialog from "$lib/thread/MoveToWorktreeDialog.svelte";
import { errorMessage } from "$lib/utils/errors";
import { flashFlag } from "$lib/utils/flash.svelte";
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

const linkCopied = flashFlag();

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

async function openContinueInTerminal() {
  if (!threadId) return;
  try {
    const command = await commands.handoffCommand(threadId, cwd);
    openDialog(HandoffConfirmDialog, {
      home,
      threadId,
      dir: cwd,
      command,
      copy: commands.copyText,
      launch: () => commands.handoffLaunchTerminal(threadId, cwd),
    });
  } catch (cause) {
    onError(errorMessage(cause));
  }
}

async function copyThreadLink() {
  if (!threadId) return;
  try {
    const link = await commands.handoffThreadLink(threadId, cwd, null);
    await commands.copyText(link);
    linkCopied.trigger();
  } catch (cause) {
    onError(errorMessage(cause));
  }
}

const inTempWorktree = $derived(isTempWorktreePath(cwd));

/** Check the temporary worktree's branch out locally and continue the thread there. */
function openHandoffToLocal() {
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
      await commands.gitWorktreeHandoff(cwd, targetDir, commitUncommitted, branchName);
      refreshGitStatus(targetDir);
      const forked = await forkThread(thread, undefined, undefined, targetDir);
      onMovedToWorktree(forked.id);
    },
  });
}

function openMoveToWorktree() {
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

const handoffActions: Record<string, () => void> = {
  terminal: openContinueInTerminal,
  local: openHandoffToLocal,
  worktree: openMoveToWorktree,
  link: copyThreadLink,
};
</script>

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

  <!-- The menu claims Escape while open, so the composer never reads it as
       "interrupt the running turn". -->
  <Menu positioning={{ placement: "bottom-end" }} onSelect={({ value }) => handoffActions[value]?.()}>
    <Menu.Trigger
      disabled={!enabled}
      title={enabled ? "Handoff options" : disabledHint}
      class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-medium text-surface-600-400 transition hover:bg-surface-200-800 hover:text-surface-800-200 disabled:cursor-not-allowed disabled:opacity-40"
    >
      <Send size={12} />
      Handoff
      <ChevronDown size={11} />
    </Menu.Trigger>
    <Portal>
      <Menu.Positioner>
        <Menu.Content class="popover-card z-50 w-60 select-none gap-0 p-1.5" aria-label="Handoff options">
          <Menu.Item value="terminal" class="menu-row">
            <TerminalSquare size={14} class="shrink-0 text-surface-500" />
            <Menu.ItemText class="min-w-0 flex-1">Continue in terminal</Menu.ItemText>
          </Menu.Item>
          {#if inTempWorktree}
            <Menu.Item value="local" class="menu-row">
              <ArrowLeftRight size={14} class="shrink-0 text-surface-500" />
              <Menu.ItemText class="min-w-0 flex-1">Hand off to local</Menu.ItemText>
            </Menu.Item>
          {/if}
          <Menu.Item value="worktree" class="menu-row">
            <GitBranch size={14} class="shrink-0 text-surface-500" />
            <Menu.ItemText class="min-w-0 flex-1">Move to worktree</Menu.ItemText>
          </Menu.Item>
          <Menu.Item value="link" class="menu-row">
            {#if linkCopied.on}<Copy size={14} class="shrink-0 text-primary-500" />{:else}<Link2 size={14} class="shrink-0 text-surface-500" />{/if}
            <Menu.ItemText class="min-w-0 flex-1">{linkCopied.on ? "Link copied" : "Copy thread link"}</Menu.ItemText>
          </Menu.Item>
        </Menu.Content>
      </Menu.Positioner>
    </Portal>
  </Menu>
</div>
