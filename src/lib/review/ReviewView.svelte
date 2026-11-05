<script lang="ts">
  import { ArrowLeft, Bot, FileDiff, GitBranch, GitPullRequest, LogIn, RefreshCw } from "@lucide/svelte";
  import { onMount } from "svelte";
  import DiffBlock from "$lib/components/DiffBlock.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { inboxes } from "$lib/inbox/inboxStore.svelte";
  import ReviewPage from "$lib/review/ReviewPage.svelte";
  import { changeStat, fileChange, reviewPrompt } from "$lib/review/review";
  import { type AskAgentRequest, askAgentPrompt } from "$lib/review/reviewPage";
  import { commands } from "$lib/services/homeRouting";
  import type { PrFile, ProviderStatus, PrSummary } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";

  let {
    repoDir,
    repoName,
    initialPr = null,
    onBack,
    onAskCodex,
  }: {
    repoDir: string;
    repoName: string;
    /** Open straight onto this PR's review page. */
    initialPr?: number | null;
    onBack: () => void;
    onAskCodex: (cwd: string, prompt: string) => void;
  } = $props();

  // picker → choosing a PR or a local base; pr → the review page; local → branch diff.
  // svelte-ignore state_referenced_locally -- the starting PR only.
  let prNumber = $state<number | null>(initialPr);
  // svelte-ignore state_referenced_locally -- likewise.
  let mode = $state<"picker" | "pr" | "local">(prNumber === null ? "picker" : "pr");
  let provider = $state<ProviderStatus | null>(null);
  let prs = $state<PrSummary[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let localFiles = $state<PrFile[]>([]);
  let localBase = $state("main");
  let selectedPath = $state<string | null>(null);
  const selectedFile = $derived<PrFile | null>(
    localFiles.find((file) => file.path === selectedPath) ?? localFiles[0] ?? null,
  );

  async function loadProvider() {
    loading = true;
    error = null;
    try {
      provider = await commands.reviewProviderStatus(repoDir);
      if (provider.installed && provider.authenticated) {
        prs = await commands.reviewListPrs(repoDir);
      }
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  onMount(loadProvider);

  function openPr(number: number) {
    prNumber = number;
    mode = "pr";
  }

  async function openLocal() {
    loading = true;
    error = null;
    try {
      localFiles = await commands.reviewLocalDiff(repoDir, localBase, null);
      mode = "local";
      selectedPath = localFiles[0]?.path ?? null;
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  /** Until agent reviews answer in their own thread, Ask agent pre-fills a
   *  new thread's composer in the repository. */
  function askAgent(request: AskAgentRequest) {
    onAskCodex(repoDir, askAgentPrompt(request));
  }

  function askAboutLocal() {
    onAskCodex(repoDir, reviewPrompt(`Local changes vs ${localBase}`, localBase, "working tree", localFiles));
  }

  function backToPicker() {
    mode = "picker";
    prNumber = null;
    localFiles = [];
    selectedPath = null;
  }
</script>

{#if mode === "pr" && prNumber !== null}
  {#key prNumber}
    <ReviewPage {repoDir} number={prNumber} onBack={backToPicker} onOpenPr={openPr} onAskAgent={askAgent} />
  {/key}
{:else}
  <div class="flex h-full flex-col">
    <div class="flex items-center gap-2 border-b border-surface-200-800 px-4 py-2.5">
      <TooltipButton
        label="Back"
        onclick={mode === "picker" ? onBack : backToPicker}
        aria-label="Back"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      >
        <ArrowLeft size={16} />
      </TooltipButton>
      <GitPullRequest size={16} class="shrink-0 text-primary-500" />
      <div class="min-w-0 flex-1 truncate text-sm font-semibold">
        {mode === "local" ? `Local changes vs ${localBase}` : `Review · ${repoName}`}
      </div>
      <TooltipButton
        label="Refresh"
        onclick={mode === "local" ? openLocal : loadProvider}
        aria-label="Refresh"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      >
        <RefreshCw size={15} class={loading ? "animate-spin" : ""} />
      </TooltipButton>
    </div>

    {#if error}
      <div class="grid flex-1 place-items-center p-8">
        <div class="card preset-tonal-error max-w-md p-4 text-sm">
          <div class="font-semibold">Could not load review data</div>
          <p class="mt-1 text-xs leading-5">{error}</p>
        </div>
      </div>
    {:else if provider && (!provider.installed || !provider.authenticated) && mode === "picker"}
      <div class="grid flex-1 place-items-center p-8">
        <div class="card preset-tonal max-w-md p-5 text-center text-sm">
          <LogIn size={22} class="mx-auto text-surface-500" />
          <div class="mt-2 font-semibold">
            {provider.installed ? "GitHub CLI not signed in" : "GitHub CLI not found"}
          </div>
          <p class="mt-1 text-xs leading-5 text-surface-500">{provider.message}</p>
          <button onclick={loadProvider} class="btn btn-sm mt-3 preset-tonal-primary">Try again</button>
        </div>
      </div>
    {:else if mode === "picker"}
      {@render picker()}
    {:else}
      {@render localDiff()}
    {/if}
  </div>
{/if}

{#snippet picker()}
  <div class="flex-1 overflow-y-auto px-6 py-5">
    <div class="mx-auto max-w-2xl">
      <h2 class="eyebrow">Open pull requests</h2>
      {#if loading && prs.length === 0}
        <div class="mt-2 space-y-2">
          <div class="placeholder h-14 animate-pulse rounded-xl"></div>
          <div class="placeholder h-14 animate-pulse rounded-xl opacity-70"></div>
        </div>
      {:else if prs.length === 0}
        <p class="mt-2 empty-state">No open pull requests for this repository.</p>
      {:else}
        <ul class="mt-2 space-y-2">
          {#each prs as pr (pr.number)}
            <li>
              <button
                onclick={() => openPr(pr.number)}
                class="flex w-full items-start gap-3 panel-card p-3 text-left hover:border-primary-500/40"
              >
                <GitPullRequest size={15} class="mt-0.5 shrink-0 text-primary-500" />
                <div class="min-w-0 flex-1">
                  <div class="truncate text-sm font-medium">#{pr.number} {pr.title}</div>
                  <div class="mt-0.5 flex flex-wrap items-center gap-x-2 text-[11px] text-surface-500">
                    <span>{pr.author}</span>
                    <span class="inline-flex items-center gap-1"
                      ><GitBranch size={10} />{pr.headRef}
                      → {pr.baseRef}</span
                    >
                    {#if pr.isDraft}
                      <span class="rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px]">draft</span>
                    {/if}
                  </div>
                </div>
              </button>
            </li>
          {/each}
        </ul>
      {/if}

      <h2 class="mt-6 eyebrow">Local diff (no PR)</h2>
      <div class="mt-2 flex items-center gap-2 panel-card p-3">
        <label class="text-xs text-surface-500" for="review-base">Base</label>
        <input id="review-base" bind:value={localBase} class="input input-sm w-40 text-xs" placeholder="main">
        <button onclick={openLocal} class="btn btn-sm preset-tonal-primary">
          <FileDiff size={13} />
          Diff working tree
        </button>
      </div>
    </div>
  </div>
{/snippet}

{#snippet localDiff()}
  <div class="flex min-h-0 flex-1">
    <aside class="flex w-64 shrink-0 flex-col border-r border-surface-200-800">
      <div class="border-b border-surface-200-800 px-3 py-3 text-xs text-surface-500">Working tree vs {localBase}</div>
      <ul class="min-h-0 flex-1 overflow-y-auto py-1">
        {#each localFiles as file (file.path)}
          <li>
            <button
              onclick={() => (selectedPath = file.path)}
              class="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs hover:preset-tonal {selectedFile?.path ===
              file.path
                ? "bg-primary-500/10"
                : ""}"
            >
              <span class="min-w-0 flex-1 truncate font-mono">{file.path}</span>
              <span class="shrink-0 text-[10px] text-surface-500">{changeStat(file)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </aside>
    <div class="min-w-0 flex-1 overflow-y-auto px-4 py-4">
      {#if selectedFile}
        {#if selectedFile.patchTruncated}
          <div class="panel-card p-4 text-center text-xs text-surface-500">
            No text diff available (binary or too large).
          </div>
        {:else}
          {#key selectedFile.path}
            <DiffBlock change={fileChange(selectedFile)} />
          {/key}
        {/if}
      {:else}
        <div class="grid h-full place-items-center text-xs text-surface-500">No changed files.</div>
      {/if}
    </div>
    <aside class="flex w-80 shrink-0 flex-col border-l border-surface-200-800">
      <div class="flex items-center justify-between border-b border-surface-200-800 px-4 py-3">
        <h2 class="text-sm font-semibold">Review</h2>
        <button onclick={askAboutLocal} class="btn btn-sm preset-tonal-primary"><Bot size={14} /> Ask Codex</button>
      </div>
      <p class="p-4 text-xs leading-5 text-surface-500">
        This is a local diff with no pull request yet. Push a branch and open a PR to publish review comments, or ask
        Codex to review the working-tree changes.
      </p>
    </aside>
  </div>
{/snippet}
