<script lang="ts">
import {
  AlertTriangle,
  ArrowLeft,
  Bot,
  FileDiff,
  GitBranch,
  GitPullRequest,
  LogIn,
  MessageSquarePlus,
  RefreshCw,
  X,
} from "@lucide/svelte";
import { onMount } from "svelte";
import DiffBlock from "$lib/components/DiffBlock.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import ReviewPanel from "$lib/review/ReviewPanel.svelte";
import {
  addableLines,
  changeStat,
  checksFailing,
  checksLabel,
  fileChange,
  reviewPrompt,
  staleBanner,
} from "$lib/review/review";
import {
  reviewCheckFresh,
  reviewDeleteDraft,
  reviewListPrs,
  reviewLoadDraft,
  reviewLocalDiff,
  reviewPrDetail,
  reviewProviderStatus,
  reviewReply,
  reviewResolveThread,
  reviewSaveDraft,
  reviewSubmit,
} from "$lib/services/api";
import type { PendingComment, PrDetail, PrFile, PrFreshness, ProviderStatus, PrSummary } from "$lib/types";

let {
  repoDir,
  repoName,
  onBack,
  onAskCodex,
}: {
  repoDir: string;
  repoName: string;
  onBack: () => void;
  onAskCodex: (cwd: string, prompt: string) => void;
} = $props();

const PROVIDER = "github";

// picker → choosing a PR or a local base; pr → an open PR; local → branch diff.
let mode = $state<"picker" | "pr" | "local">("picker");
let provider = $state<ProviderStatus | null>(null);
let prs = $state<PrSummary[]>([]);
let loading = $state(true);
let error = $state<string | null>(null);
let actionError = $state<string | null>(null);
let busy = $state(false);

let detail = $state<PrDetail | null>(null);
let localFiles = $state<PrFile[]>([]);
let localBase = $state("main");
let selectedPath = $state<string | null>(null);
let viewedPaths = $state<Set<string>>(new Set());

let pending = $state<PendingComment[]>([]);
let reviewStarted = $state(false);
let freshness = $state<PrFreshness | null>(null);

// Inline "add comment" composer state for the center pane.
let addingOn = $state<string | null>(null);
let addLineKey = $state("");
let addBody = $state("");

const files = $derived<PrFile[]>(mode === "local" ? localFiles : (detail?.files ?? []));
const selectedFile = $derived<PrFile | null>(files.find((file) => file.path === selectedPath) ?? files[0] ?? null);
const staleText = $derived(staleBanner(freshness));

async function loadProvider() {
  loading = true;
  error = null;
  try {
    provider = await reviewProviderStatus(repoDir);
    if (provider.installed && provider.authenticated) {
      prs = await reviewListPrs(repoDir);
    }
  } catch (cause) {
    error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading = false;
  }
}

onMount(loadProvider);

async function openPr(number: number) {
  loading = true;
  error = null;
  actionError = null;
  try {
    detail = await reviewPrDetail(repoDir, number);
    mode = "pr";
    selectedPath = detail.files[0]?.path ?? null;
    viewedPaths = new Set();
    reviewStarted = false;
    freshness = null;
    await restoreDraft(number, detail.headSha);
  } catch (cause) {
    error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading = false;
  }
}

async function openLocal() {
  loading = true;
  error = null;
  actionError = null;
  try {
    localFiles = await reviewLocalDiff(repoDir, localBase);
    mode = "local";
    selectedPath = localFiles[0]?.path ?? null;
    viewedPaths = new Set();
    pending = [];
    reviewStarted = false;
  } catch (cause) {
    error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    loading = false;
  }
}

async function restoreDraft(number: number, headSha: string) {
  pending = [];
  try {
    const draft = await reviewLoadDraft(PROVIDER, repoName, number);
    if (draft && draft.headSha === headSha) {
      const parsed = JSON.parse(draft.payload) as { pending?: PendingComment[]; reviewStarted?: boolean };
      pending = parsed.pending ?? [];
      reviewStarted = parsed.reviewStarted ?? false;
    }
  } catch {
    // A malformed or absent draft is not fatal.
  }
}

async function persistDraft() {
  if (mode !== "pr" || !detail) return;
  const payload = JSON.stringify({ pending, reviewStarted });
  await reviewSaveDraft(PROVIDER, repoName, detail.summary.number, detail.headSha, payload).catch(() => {});
}

async function refresh() {
  if (mode === "pr" && detail) {
    await openPr(detail.summary.number);
  } else if (mode === "local") {
    await openLocal();
  } else {
    await loadProvider();
  }
}

async function checkFresh() {
  if (mode !== "pr" || !detail) return;
  try {
    freshness = await reviewCheckFresh(repoDir, detail.summary.number, detail.headSha, detail.summary.updatedAt);
  } catch {
    // Freshness is best-effort; a failure just leaves the banner hidden.
  }
}

function toggleViewed(path: string) {
  const next = new Set(viewedPaths);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  viewedPaths = next;
}

function beginAddComment(path: string) {
  addingOn = addingOn === path ? null : path;
  addLineKey = "";
  addBody = "";
}

function addComment(file: PrFile) {
  const options = addableLines(file);
  const chosen = options.find((option) => option.anchor === addLineKey) ?? options[0];
  const body = addBody.trim();
  if (!chosen || !body) return;
  pending = [...pending, { path: file.path, line: chosen.line, side: chosen.side, body }];
  reviewStarted = true;
  addingOn = null;
  addBody = "";
  void persistDraft();
}

function removePending(index: number) {
  pending = pending.filter((_, currentIndex) => currentIndex !== index);
  void persistDraft();
}

function startReview() {
  reviewStarted = true;
  void persistDraft();
}

async function submitReview(event: string, body: string) {
  if (mode !== "pr" || !detail) return;
  busy = true;
  actionError = null;
  try {
    await reviewSubmit(repoDir, detail.summary.number, event, body, pending);
    pending = [];
    reviewStarted = false;
    await reviewDeleteDraft(PROVIDER, repoName, detail.summary.number).catch(() => {});
    await openPr(detail.summary.number);
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy = false;
  }
}

async function reply(commentId: number, body: string) {
  if (mode !== "pr" || !detail) return;
  busy = true;
  actionError = null;
  try {
    await reviewReply(repoDir, detail.summary.number, commentId, body);
    await openPr(detail.summary.number);
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy = false;
  }
}

async function resolve(threadId: string) {
  if (mode !== "pr" || !detail) return;
  busy = true;
  actionError = null;
  try {
    await reviewResolveThread(repoDir, threadId);
    await openPr(detail.summary.number);
  } catch (cause) {
    actionError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    busy = false;
  }
}

function askCodex() {
  const title = mode === "pr" && detail ? detail.summary.title : `Local changes vs ${localBase}`;
  const base = mode === "pr" && detail ? detail.summary.baseRef : localBase;
  const head = mode === "pr" && detail ? detail.summary.headRef : "working tree";
  onAskCodex(repoDir, reviewPrompt(title, base, head, files));
}

function backToPicker() {
  mode = "picker";
  detail = null;
  localFiles = [];
  selectedPath = null;
  freshness = null;
}
</script>

<div class="flex h-full flex-col">
  <!-- Header -->
  <div class="flex items-center gap-2 border-b border-surface-200-800 px-4 py-2.5">
    <TooltipButton label="Back" onclick={mode === "picker" ? onBack : backToPicker} aria-label="Back" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
      <ArrowLeft size={16} />
    </TooltipButton>
    <GitPullRequest size={16} class="shrink-0 text-primary-500" />
    <div class="min-w-0 flex-1">
      <div class="truncate text-sm font-semibold">
        {#if mode === "pr" && detail}
          #{detail.summary.number} {detail.summary.title}
        {:else if mode === "local"}
          Local changes vs {localBase}
        {:else}
          Review · {repoName}
        {/if}
      </div>
    </div>
    {#if mode === "pr"}
      <button onclick={checkFresh} class="btn btn-sm preset-tonal" title="Check for remote changes">Check freshness</button>
    {/if}
    <TooltipButton label="Refresh" onclick={refresh} aria-label="Refresh" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
      <RefreshCw size={15} class={loading ? "animate-spin" : ""} />
    </TooltipButton>
  </div>

  {#if staleText}
    <div class="flex items-center gap-2 border-b border-warning-500/40 bg-warning-500/10 px-4 py-2 text-xs text-warning-700 dark:text-warning-300">
      <AlertTriangle size={14} class="shrink-0" />
      <span class="flex-1">{staleText}</span>
      <button onclick={refresh} class="btn btn-sm preset-tonal">Refresh</button>
      <TooltipButton label="Dismiss" onclick={() => (freshness = null)} aria-label="Dismiss" class="btn-icon btn-icon-sm hover:preset-tonal"><X size={13} /></TooltipButton>
    </div>
  {/if}

  {#if actionError}
    <div class="border-b border-error-500/40 bg-error-500/10 px-4 py-2 text-xs text-error-600 dark:text-error-400">{actionError}</div>
  {/if}

  <!-- Body -->
  {#if error}
    <div class="grid flex-1 place-items-center p-8">
      <div class="card preset-tonal-error max-w-md p-4 text-sm">
        <div class="font-semibold">Could not load review data</div>
        <p class="mt-1 text-xs leading-5">{error}</p>
      </div>
    </div>
  {:else if provider && (!provider.installed || !provider.authenticated)}
    <div class="grid flex-1 place-items-center p-8">
      <div class="card preset-tonal max-w-md p-5 text-center text-sm">
        <LogIn size={22} class="mx-auto text-surface-500" />
        <div class="mt-2 font-semibold">{provider.installed ? "GitHub CLI not signed in" : "GitHub CLI not found"}</div>
        <p class="mt-1 text-xs leading-5 text-surface-500">{provider.message}</p>
        <button onclick={loadProvider} class="btn btn-sm mt-3 preset-tonal-primary">Try again</button>
      </div>
    </div>
  {:else if mode === "picker"}
    {@render picker()}
  {:else}
    {@render threePane()}
  {/if}
</div>

{#snippet picker()}
  <div class="flex-1 overflow-y-auto px-6 py-5">
    <div class="mx-auto max-w-2xl">
      <h2 class="text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Open pull requests</h2>
      {#if loading && prs.length === 0}
        <div class="mt-2 space-y-2">
          <div class="placeholder h-14 animate-pulse rounded-xl"></div>
          <div class="placeholder h-14 animate-pulse rounded-xl opacity-70"></div>
        </div>
      {:else if prs.length === 0}
        <p class="mt-2 rounded-xl border border-dashed border-surface-300-700 px-4 py-6 text-center text-xs text-surface-500">
          No open pull requests for this repository.
        </p>
      {:else}
        <ul class="mt-2 space-y-2">
          {#each prs as pr (pr.number)}
            <li>
              <button onclick={() => openPr(pr.number)} class="flex w-full items-start gap-3 rounded-xl border border-surface-200-800 bg-surface-100-900 p-3 text-left hover:border-primary-500/40">
                <GitPullRequest size={15} class="mt-0.5 shrink-0 text-primary-500" />
                <div class="min-w-0 flex-1">
                  <div class="truncate text-sm font-medium">#{pr.number} {pr.title}</div>
                  <div class="mt-0.5 flex flex-wrap items-center gap-x-2 text-[11px] text-surface-500">
                    <span>{pr.author}</span>
                    <span class="inline-flex items-center gap-1"><GitBranch size={10} />{pr.headRef} → {pr.baseRef}</span>
                    {#if pr.isDraft}<span class="rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px]">draft</span>{/if}
                  </div>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <h2 class="mt-6 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Local diff (no PR)</h2>
      <div class="mt-2 flex items-center gap-2 rounded-xl border border-surface-200-800 bg-surface-100-900 p-3">
        <label class="text-xs text-surface-500" for="review-base">Base</label>
        <input id="review-base" bind:value={localBase} class="input input-sm w-40 text-xs" placeholder="main" />
        <button onclick={openLocal} class="btn btn-sm preset-tonal-primary">
          <FileDiff size={13} /> Diff working tree
        </button>
      </div>
    </div>
  </div>
{/snippet}

{#snippet threePane()}
  <div class="flex min-h-0 flex-1">
    <!-- Left: summary + file list -->
    <aside class="flex w-64 shrink-0 flex-col border-r border-surface-200-800">
      <div class="border-b border-surface-200-800 px-3 py-3 text-xs">
        {#if mode === "pr" && detail}
          <div class="flex items-center gap-1.5 text-surface-500">
            <GitBranch size={11} /><span class="truncate">{detail.summary.headRef} → {detail.summary.baseRef}</span>
          </div>
          <div class="mt-1 text-surface-500">by {detail.summary.author}</div>
          {#if checksLabel(detail.checks)}
            <div class="mt-1 {checksFailing(detail.checks) ? 'text-error-500' : 'text-surface-500'}">Checks: {checksLabel(detail.checks)}</div>
          {/if}
          {#if detail.filesTruncated}
            <div class="mt-1 text-warning-600 dark:text-warning-400">File list truncated.</div>
          {/if}
        {:else}
          <div class="text-surface-500">Working tree vs {localBase}</div>
        {/if}
      </div>
      <ul class="min-h-0 flex-1 overflow-y-auto py-1">
        {#each files as file (file.path)}
          <li>
            <button
              onclick={() => (selectedPath = file.path)}
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:preset-tonal {selectedPath === file.path ? 'bg-primary-500/10' : ''}"
            >
              <input
                type="checkbox"
                checked={viewedPaths.has(file.path)}
                onclick={(event) => { event.stopPropagation(); toggleViewed(file.path); }}
                aria-label="Mark {file.path} viewed"
                class="checkbox checkbox-sm shrink-0"
              />
              <span class="min-w-0 flex-1 truncate font-mono {viewedPaths.has(file.path) ? 'text-surface-500 line-through' : ''}">{file.path}</span>
              <span class="shrink-0 text-[10px] text-surface-500">{changeStat(file)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </aside>

    <!-- Center: diff -->
    <div class="min-w-0 flex-1 overflow-y-auto px-4 py-4">
      {#if selectedFile}
        <div class="flex items-center justify-between gap-2">
          <span class="truncate font-mono text-xs text-surface-500">{selectedFile.path}</span>
          <button onclick={() => beginAddComment(selectedFile.path)} class="btn btn-sm preset-tonal">
            <MessageSquarePlus size={13} /> Add comment
          </button>
        </div>

        {#if addingOn === selectedFile.path}
          {@const lines = addableLines(selectedFile)}
          <div class="mt-2 rounded-lg border border-primary-500/30 bg-primary-500/5 p-3">
            <label class="text-[11px] text-surface-500" for="add-line">Line</label>
            <select id="add-line" bind:value={addLineKey} class="select select-sm mt-1 w-full font-mono text-[11px]">
              {#each lines as line (line.anchor)}
                <option value={line.anchor}>{line.label}</option>
              {/each}
            </select>
            <textarea bind:value={addBody} aria-label="Comment" rows="2" placeholder="Leave a comment on this line…" class="mt-2 w-full resize-y rounded-md border border-surface-300-700 bg-surface-50-950 p-2 text-xs"></textarea>
            <div class="mt-1 flex justify-end gap-1">
              <button onclick={() => (addingOn = null)} class="btn btn-sm preset-tonal">Cancel</button>
              <button onclick={() => addComment(selectedFile)} disabled={!addBody.trim() || lines.length === 0} class="btn btn-sm preset-filled-primary-500">Add</button>
            </div>
          </div>
        {/if}

        <div class="mt-3">
          {#if selectedFile.patchTruncated}
            <div class="rounded-xl border border-surface-200-800 bg-surface-100-900 p-4 text-center text-xs text-surface-500">
              No text diff available (binary or too large).
            </div>
          {:else}
            {#key selectedFile.path}
              <DiffBlock change={fileChange(selectedFile)} />
            {/key}
          {/if}
        </div>
      {:else}
        <div class="grid h-full place-items-center text-xs text-surface-500">No changed files.</div>
      {/if}
    </div>

    <!-- Right: review panel -->
    <aside class="w-80 shrink-0 border-l border-surface-200-800">
      {#if mode === "pr" && detail}
        <ReviewPanel
          comments={detail.comments}
          {pending}
          {reviewStarted}
          {busy}
          onStartReview={startReview}
          onSubmit={submitReview}
          onReply={reply}
          onResolve={resolve}
          onRemovePending={removePending}
          onAskCodex={askCodex}
        />
      {:else}
        <div class="flex h-full flex-col">
          <div class="flex items-center justify-between border-b border-surface-200-800 px-4 py-3">
            <h2 class="text-sm font-semibold">Review</h2>
            <button onclick={askCodex} class="btn btn-sm preset-tonal-primary"><Bot size={14} /> Ask Codex</button>
          </div>
          <p class="p-4 text-xs leading-5 text-surface-500">
            This is a local diff with no pull request yet. Push a branch and open a PR to publish review comments, or ask Codex to review the working-tree changes.
          </p>
        </div>
      {/if}
    </aside>
  </div>
{/snippet}
