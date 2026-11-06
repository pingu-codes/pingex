<script lang="ts">
  import { AlertTriangle, ArrowLeftRight, Check, GitBranch, Undo2 } from "@lucide/svelte";
  import { onMount, untrack } from "svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import RefreshIcon from "$lib/components/RefreshIcon.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { type CodexEvent, setThreadHandler } from "$lib/services/codexEvents.svelte";
  import { commands } from "$lib/services/homeRouting";
  import type { CommitResult, FileDiff, GitContext, SyncResult } from "$lib/types";
  import { formatBytes } from "$lib/utils/format";
  import { highlightDiff } from "$lib/utils/markdown";
  import { resource } from "$lib/utils/resource.svelte";
  import { gitStatusCache, refreshGitStatus } from "$lib/worktrees/gitStatus.svelte";
  import { aheadBehindLabel } from "$lib/worktrees/worktrees";
  import BranchSwitcherDialog from "./BranchSwitcherDialog.svelte";
  import CommitBox from "./CommitBox.svelte";
  import GitErrorNote from "./GitErrorNote.svelte";
  import { type GitError, parseGitError } from "./gitErrors";
  import { groupStatusFiles, type Selection, selectionAfterRefresh } from "./gitStatusFiles";
  import StatusFileList from "./StatusFileList.svelte";
  import type { SyncOperation } from "./SyncActions.svelte";
  import SyncActions from "./SyncActions.svelte";

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

  let selected = $state<Selection | null>(null);
  // Only the newest request lands, so a slow diff for a row the user has left
  // never shows under another row's header.
  const fileDiff = resource(
    (dir: string, current: Selection): Promise<FileDiff | null> =>
      current.section === "staged"
        ? commands.gitStagedFileDiff(dir, current.path, null)
        : commands.gitFileDiff(dir, "HEAD", current.path, current.section === "untracked", null),
    { initial: null, describe: (cause) => parseGitError(cause).message },
  );
  const diff = $derived(fileDiff.value);
  const diffLoading = $derived(fileDiff.loading);
  const diffError = $derived(fileDiff.error);

  /** One mutation at a time; the label drives button states. */
  let busy = $state<"files" | "commit" | SyncOperation | null>(null);
  let actionError = $state<GitError | null>(null);
  let commitError = $state<GitError | null>(null);
  let lastCommit = $state<CommitResult | null>(null);
  let lastSync = $state<SyncResult | null>(null);

  const status = $derived(gitStatusCache.byPath[dir] ?? null);
  const loading = $derived(gitStatusCache.loading[dir] ?? false);
  const groups = $derived(groupStatusFiles(status?.files ?? []));
  const branchLabel = $derived(
    status?.detached ? "detached HEAD" : (status?.branch ?? context.branch ?? "(no branch)"),
  );
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
  // A refresh of the same row keeps its diff on screen while reloading; a new
  // row clears it, so one file's patch never sits under another's header
  // (where it could be staged or discarded by mistake).
  let diffFor: string | null = null;
  $effect(() => {
    const current = selected;
    const snapshot = status?.refreshedAt;
    void snapshot;
    if (!current) {
      fileDiff.set(null);
      diffFor = null;
      return;
    }
    const key = `${current.section}:${current.path}`;
    if (key !== diffFor) {
      fileDiff.set(null);
      diffFor = key;
    }
    void fileDiff.reload(dir, current);
  });

  const rendered = $derived(diff ? highlightDiff(diff.patch) : "");

  /** Runs one git operation, refreshes, and reports a failure to `setError` (the action banner by default). */
  async function mutate(
    kind: NonNullable<typeof busy>,
    work: () => Promise<unknown>,
    after: "status" | "context" = "status",
    setError: (error: GitError | null) => void = (error) => (actionError = error),
  ) {
    if (busy) return false;
    busy = kind;
    setError(null);
    try {
      await work();
      await refresh();
      if (after === "context") onContextChanged?.(await commands.gitContext(dir));
      return true;
    } catch (cause) {
      setError(parseGitError(cause));
      return false;
    } finally {
      busy = null;
    }
  }

  const stage = (paths: string[]) => void mutate("files", () => commands.gitStage(dir, paths));
  const unstage = (paths: string[]) => void mutate("files", () => commands.gitUnstage(dir, paths));

  async function discard(paths: string[], untrackedPaths: string[]) {
    const total = paths.length + untrackedPaths.length;
    const plural = (n: number) => (n === 1 ? "" : "s");
    const confirmed = await openDialog(ConfirmDialog, {
      title: "Discard changes",
      body: [
        "This cannot be undone.",
        paths.length > 0 ? `Changes to ${paths.length} tracked file${plural(paths.length)} will be reverted.` : "",
        untrackedPaths.length > 0
          ? `${untrackedPaths.length} untracked file${plural(untrackedPaths.length)} will be deleted.`
          : "",
      ]
        .filter(Boolean)
        .join(" "),
      detail: [...paths, ...untrackedPaths],
      confirmLabel: `Discard ${total} file${plural(total)}`,
      icon: Undo2,
      width: 440,
    });
    if (confirmed) void mutate("files", () => commands.gitDiscard(dir, paths, untrackedPaths));
  }

  const commit = (message: string) =>
    mutate(
      "commit",
      async () => (lastCommit = await commands.gitCommit(dir, message)),
      "context",
      (error) => (commitError = error),
    );

  function sync(operation: SyncOperation) {
    lastSync = null;
    const work = async () => {
      lastSync =
        operation === "fetch"
          ? await commands.gitFetch(dir)
          : operation === "pull"
            ? await commands.gitPull(dir)
            : await commands.gitPush(dir, operation === "publish");
    };
    void mutate(operation, work, "context");
  }

  async function switchBranch() {
    const branch = await openDialog(BranchSwitcherDialog, { dir, currentBranch: status?.branch ?? context.branch });
    if (branch) {
      await refresh();
      onContextChanged?.(await commands.gitContext(dir));
    }
  }
</script>

<div class="space-y-4">
  <!-- Branch context and sync -->
  <div class="flex flex-wrap items-center gap-2 panel-card px-3 py-2">
    <div class="flex min-w-0 flex-1 items-center gap-2 text-sm">
      <GitBranch size={14} class="shrink-0 text-primary-500" />
      <span class="truncate font-mono font-medium">{branchLabel}</span>
      {#if status?.upstream}
        <span class="truncate text-[11px] text-surface-500" title="Upstream">→ {status.upstream}</span>
      {:else if status && !status.detached}
        <span class="text-[11px] text-surface-500">no upstream</span>
      {/if}
      {#if aheadBehind}
        <span class="font-mono text-[11px] text-surface-500">{aheadBehind}</span>
      {/if}
      {#if context.inProgress}
        <span
          class="inline-flex items-center gap-1 rounded-full bg-warning-500/15 px-1.5 py-0.5 text-[10px] font-medium text-warning-700 dark:text-warning-300"
          ><AlertTriangle size={10} /> {context.inProgress} in progress</span
        >
      {/if}
    </div>
    <button
      type="button"
      onclick={switchBranch}
      disabled={busy !== null}
      class="btn btn-sm preset-tonal disabled:opacity-40"
    >
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
    <TooltipButton
      label="Refresh status"
      onclick={refresh}
      aria-label="Refresh status"
      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    >
      <RefreshIcon spinning={loading} size={14} />
    </TooltipButton>
  </div>

  {#if actionError}
    <GitErrorNote error={actionError} />
  {:else if lastSync}
    <div
      class="flex items-center gap-1.5 rounded-md bg-success-500/10 px-3 py-2 text-xs text-success-700 dark:text-success-300"
    >
      <Check size={12} /> {lastSync.summary}
    </div>
  {/if}

  {#if !status && loading}
    <div class="placeholder h-24 animate-pulse rounded-xl"></div>
  {:else if !status}
    <p class="empty-state">Could not read the Git status of this folder.</p>
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
          <div class="empty-state py-10">Select a file to see its diff.</div>
        {:else}
          <div class="flex items-center gap-2 pb-2 text-xs">
            <span class="min-w-0 flex-1 truncate font-mono" title={selected.path}>{selected.path}</span>
            <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] text-surface-500"
              >{selected.section === "staged"
                ? "index vs HEAD"
                : selected.section === "untracked"
                  ? "new file"
                  : "working tree"}</span
            >
          </div>
          {#if diffLoading && !diff}
            <p class="text-xs text-surface-500">Loading diff…</p>
          {:else if diffError}
            <p class="text-xs text-error-500">{diffError}</p>
          {:else if diff?.binary}
            <p class="text-xs text-surface-500">Binary file — no text diff.</p>
          {:else if diff}
            <div class="overflow-hidden rounded-xl border border-surface-200-800">
              <pre
                class="code-dark max-h-[32rem] overflow-auto px-3 py-2.5 font-mono text-[11px] leading-5"
              ><code class="hljs">{@html rendered}</code></pre>
            </div>
            {#if diff.truncated}
              <p class="mt-1 text-[11px] text-surface-500">Showing the first {formatBytes(diff.bytes)}.</p>
            {/if}
          {/if}
        {/if}
      </div>
    </div>
  {/if}
</div>
