<script lang="ts">
import { AlertTriangle, ArrowLeftRight, Check, GitBranch, RefreshCw } from "@lucide/svelte";
import { onMount, untrack } from "svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  gitCommit,
  gitContext,
  gitDiscard,
  gitFetch,
  gitFileDiff,
  gitPull,
  gitPush,
  gitStage,
  gitStagedFileDiff,
  gitUnstage,
} from "$lib/services/api";
import { type CodexEvent, setThreadHandler } from "$lib/services/codexEvents.svelte";
import type { CommitResult, FileDiff, GitContext, SyncResult } from "$lib/types";
import { highlightAs } from "$lib/utils/markdown";
import { gitStatusCache, refreshGitStatus } from "$lib/worktrees/gitStatus.svelte";
import { aheadBehindLabel } from "$lib/worktrees/worktrees";
import BranchSwitcherDialog from "./BranchSwitcherDialog.svelte";
import CommitBox from "./CommitBox.svelte";
import DiscardDialog from "./DiscardDialog.svelte";
import { type GitError, hintFor, parseGitError } from "./gitErrors";
import { groupStatusFiles, type Selection, selectionAfterRefresh } from "./gitStatusFiles";
import StatusFileList from "./StatusFileList.svelte";
import SyncActions from "./SyncActions.svelte";
import type { SyncOperation } from "./sync";

let {
  dir,
  context,
  onContextChanged,
}: {
  dir: string;
  context: GitContext;
  /** Branch or identity changed (commit, checkout); the parent re-reads context. */
  onContextChanged?: (context: GitContext) => void;
} = $props();

const HIGHLIGHT_BYTES = 100_000;

let selected = $state<Selection | null>(null);
let diff = $state<FileDiff | null>(null);
let diffLoading = $state(false);
let diffError = $state<string | null>(null);
let requestId = 0;

/** One mutation at a time; the label drives button states. */
let busy = $state<"files" | "commit" | SyncOperation | null>(null);
let actionError = $state<GitError | null>(null);
let commitError = $state<GitError | null>(null);
let lastCommit = $state<CommitResult | null>(null);
let lastSync = $state<SyncResult | null>(null);

const status = $derived(gitStatusCache.byPath[dir] ?? null);
const loading = $derived(gitStatusCache.loading[dir] ?? false);
const groups = $derived(groupStatusFiles(status?.files ?? []));
const branchLabel = $derived(status?.detached ? "detached HEAD" : (status?.branch ?? context.branch ?? "(no branch)"));
const aheadBehind = $derived(aheadBehindLabel(status?.ahead ?? 0, status?.behind ?? 0));

async function refresh() {
  await refreshGitStatus(dir);
  selected = selectionAfterRefresh(
    untrack(() => selected),
    groupStatusFiles(gitStatusCache.byPath[dir]?.files ?? []),
  );
}

onMount(() => {
  void refresh();
  return setThreadHandler((event: CodexEvent) => {
    if (event.method === "turn/completed") void refresh();
  });
});

// Load the diff for whichever row is selected; staged rows diff the index.
$effect(() => {
  const current = selected;
  const snapshot = status?.refreshedAt;
  void snapshot;
  if (!current) {
    diff = null;
    return;
  }
  const id = ++requestId;
  diffLoading = true;
  diffError = null;
  const request =
    current.section === "staged"
      ? gitStagedFileDiff(dir, current.path)
      : gitFileDiff(dir, "HEAD", current.path, current.section === "untracked");
  request
    .then((result) => {
      if (id === requestId) diff = result;
    })
    .catch((cause) => {
      if (id === requestId) diffError = parseGitError(cause).message;
    })
    .finally(() => {
      if (id === requestId) diffLoading = false;
    });
});

const rendered = $derived.by(() => {
  if (!diff) return "";
  if (diff.patch.length > HIGHLIGHT_BYTES) return escapeHtml(diff.patch);
  return highlightAs(diff.patch, "diff");
});

function escapeHtml(text: string): string {
  return text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

async function mutate(
  kind: NonNullable<typeof busy>,
  work: () => Promise<void>,
  after: "status" | "context" = "status",
) {
  if (busy) return false;
  busy = kind;
  actionError = null;
  try {
    await work();
    await refresh();
    if (after === "context") onContextChanged?.(await gitContext(dir));
    return true;
  } catch (cause) {
    actionError = parseGitError(cause);
    return false;
  } finally {
    busy = null;
  }
}

const stage = (paths: string[]) => void mutate("files", () => gitStage(dir, paths));
const unstage = (paths: string[]) => void mutate("files", () => gitUnstage(dir, paths));

async function discard(paths: string[], untrackedPaths: string[]) {
  const confirmed = await openDialog(DiscardDialog, { paths, untrackedPaths });
  if (confirmed) void mutate("files", () => gitDiscard(dir, paths, untrackedPaths));
}

async function commit(message: string): Promise<boolean> {
  if (busy) return false;
  busy = "commit";
  commitError = null;
  try {
    lastCommit = await gitCommit(dir, message);
    await refresh();
    onContextChanged?.(await gitContext(dir));
    return true;
  } catch (cause) {
    commitError = parseGitError(cause);
    return false;
  } finally {
    busy = null;
  }
}

function sync(operation: SyncOperation) {
  lastSync = null;
  const work = async () => {
    lastSync =
      operation === "fetch"
        ? await gitFetch(dir)
        : operation === "pull"
          ? await gitPull(dir)
          : await gitPush(dir, operation === "publish");
  };
  void mutate(operation, work, "context");
}

async function switchBranch() {
  const branch = await openDialog(BranchSwitcherDialog, { dir, currentBranch: status?.branch ?? context.branch });
  if (branch) {
    await refresh();
    onContextChanged?.(await gitContext(dir));
  }
}
</script>

<div class="space-y-4">
  <!-- Branch context and sync -->
  <div class="flex flex-wrap items-center gap-2 rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2">
    <div class="flex min-w-0 flex-1 items-center gap-2 text-sm">
      <GitBranch size={14} class="shrink-0 text-primary-500" />
      <span class="truncate font-mono font-medium">{branchLabel}</span>
      {#if status?.upstream}
        <span class="truncate text-[11px] text-surface-500" title="Upstream">→ {status.upstream}</span>
      {:else if status && !status.detached}
        <span class="text-[11px] text-surface-500">no upstream</span>
      {/if}
      {#if aheadBehind}<span class="font-mono text-[11px] text-surface-500">{aheadBehind}</span>{/if}
      {#if context.inProgress}
        <span class="inline-flex items-center gap-1 rounded-full bg-warning-500/15 px-1.5 py-0.5 text-[10px] font-medium text-warning-700 dark:text-warning-300"><AlertTriangle size={10} /> {context.inProgress} in progress</span>
      {/if}
    </div>
    <button type="button" onclick={switchBranch} disabled={busy !== null} class="btn btn-sm preset-tonal disabled:opacity-40">
      <ArrowLeftRight size={13} />
      Switch branch
    </button>
    <SyncActions
      upstream={status?.upstream ?? null}
      ahead={status?.ahead ?? 0}
      behind={status?.behind ?? 0}
      detached={status?.detached ?? false}
      busy={busy === "fetch" || busy === "pull" || busy === "push" || busy === "publish" ? busy : null}
      onSync={sync}
    />
    <TooltipButton label="Refresh status" onclick={refresh} aria-label="Refresh status" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
      <RefreshCw size={14} class={loading ? "animate-spin" : ""} />
    </TooltipButton>
  </div>

  {#if actionError}
    <div class="rounded-md preset-tonal-error px-3 py-2 text-xs">
      <div>{actionError.message}</div>
      {#if hintFor(actionError.kind)}<div class="mt-0.5 opacity-80">{hintFor(actionError.kind)}</div>{/if}
      {#if actionError.detail}<pre class="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[10px] leading-4 opacity-90">{actionError.detail}</pre>{/if}
    </div>
  {:else if lastSync}
    <div class="flex items-center gap-1.5 rounded-md bg-success-500/10 px-3 py-2 text-xs text-success-700 dark:text-success-300">
      <Check size={12} /> {lastSync.summary}
    </div>
  {/if}

  {#if !status && loading}
    <div class="placeholder h-24 animate-pulse rounded-xl"></div>
  {:else if !status}
    <p class="rounded-xl border border-dashed border-surface-300-700 px-4 py-6 text-center text-xs text-surface-500">Could not read the Git status of this folder.</p>
  {:else}
    <div class="grid gap-4 lg:grid-cols-[minmax(16rem,2fr)_3fr]">
      <div class="min-w-0 space-y-3">
        <StatusFileList
          files={status.files}
          {selected}
          busy={busy !== null}
          onSelect={(next) => (selected = next)}
          onStage={stage}
          onUnstage={unstage}
          onDiscard={discard}
        />
        {#if status.truncated}
          <p class="text-[11px] text-warning-500">Only the first {status.files.length} files are listed.</p>
        {/if}
        <CommitBox
          stagedCount={groups.staged.length}
          identityName={context.identityName}
          identityEmail={context.identityEmail}
          busy={busy === "commit"}
          {lastCommit}
          error={commitError}
          onCommit={commit}
        />
      </div>
      <div class="min-w-0">
        {#if !selected}
          <div class="rounded-xl border border-dashed border-surface-300-700 px-4 py-10 text-center text-xs text-surface-500">Select a file to see its diff.</div>
        {:else}
          <div class="flex items-center gap-2 pb-2 text-xs">
            <span class="min-w-0 flex-1 truncate font-mono" title={selected.path}>{selected.path}</span>
            <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] text-surface-500">{selected.section === "staged" ? "index vs HEAD" : selected.section === "untracked" ? "new file" : "working tree"}</span>
          </div>
          {#if diffLoading && !diff}
            <p class="text-xs text-surface-500">Loading diff…</p>
          {:else if diffError}
            <p class="text-xs text-error-500">{diffError}</p>
          {:else if diff?.binary}
            <p class="text-xs text-surface-500">Binary file — no text diff.</p>
          {:else if diff}
            <div class="overflow-hidden rounded-xl border border-surface-200-800">
              <pre class="diff-block max-h-[32rem] overflow-auto px-3 py-2.5 font-mono text-[11px] leading-5"><code class="hljs">{@html rendered}</code></pre>
            </div>
            {#if diff.truncated}
              <p class="mt-1 text-[11px] text-surface-500">Showing the first {Math.round(diff.bytes / 1024)} KB.</p>
            {/if}
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .diff-block {
    background: #0d1117;
    color: #e6edf3;
  }
  .diff-block :global(.hljs) {
    background: transparent;
    padding: 0;
  }
</style>
