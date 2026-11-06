<!--
  The Pull requests inbox for one repository (the canvas's "Pull requests"
  board): filters with counts, toggle chips and a text filter; my stacks as
  cards, my other PRs, review requests grouped by the author's stack, then
  everyone else; a "Since you last looked" feed and the agent reviews in
  flight. Rows multi-select for batch actions.

  The classification behind the filters is the backend's (`inbox.triage`).
  Actions that open another screen go to `handlers`; the rest run here.
-->
<script lang="ts">
  import { Check, ChevronDown, RefreshCw, Search, X } from "@lucide/svelte";
  import { Menu, Portal } from "@skeletonlabs/skeleton-svelte";
  import { onMount } from "svelte";
  import { projects } from "$lib/app/appData.svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import AgentReviewsPanel from "$lib/inbox/AgentReviewsPanel.svelte";
  import { inFlight, latestPerPr, loadAgentReviews, repoAgentReviews } from "$lib/inbox/agentReviews";
  import CheckLogDialog from "$lib/inbox/CheckLogDialog.svelte";
  import type { InboxHandlers } from "$lib/inbox/handlers";
  import {
    ago,
    FILTER_LABELS,
    type InboxChips,
    type InboxFilter,
    type InboxRow,
    inboxSections,
    primaryAction,
    rangeSelection,
    type StackGroup,
    sectionOrder,
    visibleRows,
  } from "$lib/inbox/inbox";
  import { inboxes, markActivitySeen, mutePr, syncInbox } from "$lib/inbox/inboxStore.svelte";
  import { ciMenu, prMenu } from "$lib/inbox/menus";
  import PrRow from "$lib/inbox/PrRow.svelte";
  import SinceYouLooked from "$lib/inbox/SinceYouLooked.svelte";
  import StackCard from "$lib/inbox/StackCard.svelte";
  import { rerunOnNewCommits, reviewTarget } from "$lib/review/agentReviews.svelte";
  import CompareCommitsDialog from "$lib/review/CompareCommitsDialog.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { stackInputFor } from "$lib/stack/stack";
  import { toastError } from "$lib/toaster";
  import type { InboxPr } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";
  import { ensureWorktrees, worktreeMap } from "$lib/worktrees/worktreeMap.svelte";
  import { threadCountForPath } from "$lib/worktrees/worktrees";

  let {
    repos,
    repoPath,
    onSelectRepo,
    handlers,
  }: {
    /** The repositories with an inbox, by folder. */
    repos: { path: string; name: string }[];
    repoPath: string;
    onSelectRepo: (path: string) => void;
    handlers: InboxHandlers;
  } = $props();

  const GROUP_KEY = "pingex.inbox.groupByStack";
  function loadGroupByStack(): boolean {
    try {
      return localStorage.getItem(GROUP_KEY) !== "false";
    } catch {
      return true;
    }
  }

  let filter = $state<InboxFilter>("needsMe");
  let chips = $state<InboxChips>({ drafts: false, ciFailing: false, unresolved: false });
  let text = $state("");
  let groupByStack = $state(loadGroupByStack());
  let selected = $state<Set<number>>(new Set());
  let anchor = $state<number | null>(null);
  let now = $state(Date.now());

  const repoState = $derived(inboxes[repoPath]);
  const inbox = $derived(repoState?.inbox ?? null);
  const stacks = $derived(repoState?.stacks ?? { supported: false, stacks: [] });
  const repoName = $derived(repos.find((repo) => repo.path === repoPath)?.name ?? repoPath.split("/").pop());
  const rows = $derived(inbox ? visibleRows(inbox, filter, chips, text) : []);
  const sections = $derived(inbox ? inboxSections(inbox, stacks, rows, groupByStack) : []);
  const order = $derived(sectionOrder(sections));
  const worktrees = $derived(worktreeMap.byRepo[repoPath] ?? []);
  const agentRuns = $derived(repoAgentReviews(repoPath));
  const agentByNumber = $derived(
    new Map(
      latestPerPr(agentRuns)
        .filter((review) => review.status !== "failed")
        .map((review) => [review.number, review]),
    ),
  );
  const agentState = (number: number) => {
    const status = agentByNumber.get(number)?.status;
    return status === "failed" ? null : (status ?? null);
  };
  const filterOptions = $derived(
    (Object.keys(FILTER_LABELS) as InboxFilter[]).map((value) => ({
      value,
      label: `${FILTER_LABELS[value]} ${inbox?.counts[value] ?? 0}`,
    })),
  );
  // How far behind its base each of my stacks is ("Behind main by 3").
  let behind = $state<Record<number, number | null>>({});
  $effect(() => {
    const current = inbox;
    const listing = stacks;
    if (!current || !listing.supported) return;
    const authorOf = new Map(current.prs.map((pr) => [pr.number, pr.author]));
    const mine = listing.stacks.filter(
      (stack) =>
        stack.open && stack.prs.some((pr) => pr.state === "open" && authorOf.get(pr.number) === current.viewer),
    );
    if (mine.length === 0) return;
    void commands
      .reviewStacksBehind(
        repoPath,
        mine.map((stack) => stackInputFor(stack, current)),
      )
      .then((rows) => (behind = Object.fromEntries((rows ?? []).map((row) => [row.number, row.behind]))))
      .catch(() => {});
  });

  function compareBase(group: StackGroup) {
    if (!inbox) return;
    const input = stackInputFor(group.stack, inbox);
    const bottom = input.prs.find((pr) => pr.open && !pr.merged);
    if (!bottom || !input.baseSha) return;
    void openDialog(CompareCommitsDialog, {
      repoDir: repoPath,
      base: bottom.headSha,
      head: input.baseSha,
      title: `What ${input.baseRef} gained since ${group.name} forked`,
    });
  }

  const CHIPS: { key: keyof InboxChips; label: string }[] = [
    { key: "drafts", label: "Drafts" },
    { key: "ciFailing", label: "CI failing" },
    { key: "unresolved", label: "Unresolved" },
  ];

  async function sync() {
    await syncInbox(repoPath);
    now = Date.now();
    await loadAgentReviews(repoPath);
    const synced = inboxes[repoPath]?.inbox;
    if (synced)
      void rerunOnNewCommits(
        repoPath,
        synced.prs.map((pr) => reviewTarget(pr, synced.viewer)),
      );
  }

  onMount(() => {
    ensureWorktrees([repoPath]);
    if (!inboxes[repoPath]?.inbox) void sync();
    else void loadAgentReviews(repoPath);
  });

  function setGroupByStack(on: boolean) {
    groupByStack = on;
    try {
      localStorage.setItem(GROUP_KEY, String(on));
    } catch {
      // A per-viewer convenience: fine to lose.
    }
  }

  // --- Selection ---

  function pick(number: number, mode: "toggle" | "range") {
    if (mode === "range" && anchor !== null) {
      selected = rangeSelection(order, anchor, number, selected);
      return;
    }
    const next = new Set(selected);
    if (next.has(number)) next.delete(number);
    else next.add(number);
    selected = next;
    anchor = number;
  }

  function clearSelection() {
    selected = new Set();
    anchor = null;
  }

  const selectedPrs = $derived(
    order
      .filter((number) => selected.has(number))
      .flatMap((number) => inbox?.prs.find((pr) => pr.number === number) ?? []),
  );

  // --- Actions ---

  function fail(cause: unknown) {
    toastError(errorMessage(cause));
  }

  function open(pr: InboxPr, options: { changesSince?: boolean } = {}) {
    void markActivitySeen(repoPath, [pr.number]).catch(fail);
    handlers.openPr(repoPath, pr, options);
  }

  async function confirmThen(
    pr: InboxPr,
    dialog: { title: string; body: string; confirmLabel: string; tone?: "error" | "warning" },
    work: () => Promise<unknown>,
  ) {
    const ok = await openDialog(ConfirmDialog, { ...dialog, detail: `#${pr.number} ${pr.title}` });
    if (!ok) return;
    try {
      await work();
      await sync();
    } catch (cause) {
      fail(cause);
    }
  }

  async function viewLog(pr: InboxPr) {
    try {
      const logs = await commands.reviewCheckLogs(repoPath, pr.number);
      await openDialog(CheckLogDialog, { title: `Failing checks on #${pr.number}`, logs });
    } catch (cause) {
      fail(cause);
    }
  }

  function run(id: string, row: InboxRow, group: StackGroup | null) {
    const pr = row.pr;
    if (id.startsWith("worktree:")) {
      const path = id.slice("worktree:".length);
      handlers.openInWorktree?.(repoPath, pr, path === "new" ? null : path);
      return;
    }
    switch (id) {
      case "open":
      case "primary:open":
      case "primary:review":
      case "primary:respond":
        return open(pr);
      case "primary:changesSince":
        return open(pr, { changesSince: true });
      case "primary:reviewStack":
        if (group && handlers.openStack) return handlers.openStack(repoPath, group);
        return open(pr);
      case "primary:startReview":
      case "primary:agentReview":
      case "agentReview":
        return handlers.startAgentReviews?.(repoPath, [pr]);
      case "primary:fixCi":
      case "fixCi":
        return handlers.fixCi?.(repoPath, pr);
      case "primary:merge":
        return void confirmThen(
          pr,
          {
            title: "Merge pull request",
            body: "Squash and merge this pull request into its base.",
            confirmLabel: "Merge",
            tone: "warning",
          },
          () => commands.reviewMerge(repoPath, pr.number, { method: "squash", headSha: pr.headSha }),
        );
      case "close":
        return void confirmThen(
          pr,
          {
            title: "Close pull request",
            body: "Close this pull request without merging. It can be reopened on GitHub.",
            confirmLabel: "Close pull request",
          },
          () => commands.reviewClose(repoPath, pr.number),
        );
      case "newThread":
        return handlers.newThread(repoPath, pr);
      case "copyBranch":
        return void commands.copyText(pr.headRef).catch(fail);
      case "copyLink":
        return void commands.copyText(pr.url).catch(fail);
      case "openGitHub":
        return void commands.openExternalUrl(pr.url).catch(fail);
      case "seen":
        return void markActivitySeen(repoPath, [pr.number]).catch(fail);
      case "mute":
        return void mutePr(repoPath, pr.number, pr.headSha).catch(fail);
      case "rerunChecks":
        return void commands.reviewRerunFailedChecks(repoPath, pr.number).then(sync).catch(fail);
      case "viewLog":
        return void viewLog(pr);
    }
  }

  /** The primary action needs a handler a later slice wires. */
  function unwired(id: string): boolean {
    if (id === "fixCi") return !handlers.fixCi;
    if (id === "startReview" || id === "agentReview") return !handlers.startAgentReviews;
    return false;
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && selected.size > 0) clearSelection();
  }

  const syncedAgo = $derived(inbox ? ago(inbox.syncedAt, now) : "");
</script>

{#snippet prRow(row: InboxRow, depth: number, group: StackGroup | null)}
  {@const action = primaryAction(row, {
    stackAbove: group !== null && depth > 0 && !row.triage.mine,
    agentReview: agentState(row.pr.number),
  })}
  <PrRow
    {row}
    primary={action}
    primaryDisabled={unwired(action.id)}
    agentReview={agentState(row.pr.number)}
    agentFindings={agentByNumber.get(row.pr.number)?.findings ?? 0}
    selected={selected.has(row.pr.number)}
    selecting={selected.size > 0}
    menu={prMenu(row.pr, {
      worktrees,
      canOpenInWorktree: !!handlers.openInWorktree,
      canStartAgentReview: !!handlers.startAgentReviews,
    })}
    ciEntries={ciMenu({ canFixCi: !!handlers.fixCi })}
    onAction={(id) => run(id, row, group)}
    onPick={(mode) => pick(row.pr.number, mode)}
  />
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="@container/desk flex h-full min-h-0 flex-wrap content-start overflow-y-auto"
  role="region"
  aria-label="Pull requests"
  onkeydown={onKeydown}
>
  <section class="@container/inbox flex min-w-0 flex-[999_1_560px] flex-col gap-4 px-6 py-5">
    <header class="flex flex-wrap items-center gap-3">
      <div class="min-w-[200px] grow">
        <!-- With several repositories the heading is the picker. -->
        <h1 class="w-fit text-base font-semibold">
          {#if repos.length > 1}
            <Menu positioning={{ placement: "bottom-start", gutter: 4 }}>
              <Menu.Trigger
                aria-label="Repository: {repoName}"
                class="flex items-center gap-1 rounded px-1 -mx-1 hover:preset-tonal"
              >
                {repoName}<ChevronDown size={14} class="text-surface-500" aria-hidden="true" />
              </Menu.Trigger>
              <Portal>
                <Menu.Positioner>
                  <Menu.Content class="popover-card z-50 min-w-48 select-none p-1 text-xs font-normal">
                    {#each repos as repo (repo.path)}
                      <Menu.OptionItem
                        type="radio"
                        value={repo.path}
                        checked={repo.path === repoPath}
                        onCheckedChange={(checked) => checked && repo.path !== repoPath && onSelectRepo(repo.path)}
                        class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 hover:preset-tonal"
                      >
                        <Menu.ItemText class="flex-1 truncate">{repo.name}</Menu.ItemText>
                        <Menu.ItemIndicator class="hidden data-[state=checked]:block">
                          <Check size={13} class="text-primary-500" />
                        </Menu.ItemIndicator>
                      </Menu.OptionItem>
                    {/each}
                  </Menu.Content>
                </Menu.Positioner>
              </Portal>
            </Menu>
          {:else}
            {repoName}
          {/if}
        </h1>
        <div class="text-xs text-surface-500">
          {#if inbox}
            {inbox.repo}
            · synced {syncedAgo === "now" ? "just now" : `${syncedAgo} ago`}
          {:else if repoState?.syncing}
            Syncing…
          {/if}
        </div>
      </div>
      <label class="relative flex items-center">
        <Search size={14} class="absolute left-2 text-surface-500" aria-hidden="true" />
        <span class="sr-only">Filter pull requests</span>
        <input class="field w-60 pl-7 text-xs" placeholder="Filter by title, branch or author" bind:value={text}>
      </label>
      <button
        type="button"
        class="btn btn-sm text-xs preset-tonal"
        disabled={repoState?.syncing}
        onclick={() => void sync()}
      >
        <RefreshCw size={14} class={repoState?.syncing ? "animate-spin" : ""} aria-hidden="true" />Sync
      </button>
    </header>

    {#if repoState?.error && !inbox}
      <div class="error-note">{repoState.error}</div>
    {:else if inbox}
      <div class="flex flex-wrap items-center gap-2">
        <Segmented label="Show" value={filter} options={filterOptions} onChange={(value) => (filter = value)} />
        <span class="grow"></span>
        {#each CHIPS as chip (chip.key)}
          <button
            type="button"
            class={chips[chip.key] ? "pill-active" : "pill"}
            aria-pressed={chips[chip.key]}
            onclick={() => (chips = { ...chips, [chip.key]: !chips[chip.key] })}
          >
            {chip.label}
          </button>
        {/each}
        <button
          type="button"
          class={groupByStack ? "pill-active" : "pill"}
          aria-pressed={groupByStack}
          onclick={() => setGroupByStack(!groupByStack)}
        >
          Group by stack
        </button>
      </div>

      {#if repoState?.error}
        <div class="error-note">{repoState.error}</div>
      {/if}

      {#each sections as section (section.id)}
        <div class="flex items-center gap-2" class:mt-1={section.id !== sections[0].id}>
          <h2 class="eyebrow">{section.title}</h2>
          <span class="h-px grow bg-surface-200-800"></span>
        </div>
        {#if section.id === "yourStacks"}
          {#each section.items as item (item.kind === "stack"
            ? `stack-${item.group.stack.number}`
            : item.row.pr.number)}
            {#if item.kind === "stack"}
              {@const group = item.group}
              {@const worktree =
                worktrees.find((entry) => group.stack.prs.some((pr) => pr.headRef === entry.branch)) ?? null}
              <StackCard
                {group}
                mine={true}
                {worktree}
                threadCount={worktree ? threadCountForPath(projects(), worktree.path) : 0}
                onOpenStack={handlers.openStack ? () => handlers.openStack?.(repoPath, group) : undefined}
                onRestack={handlers.restack ? () => handlers.restack?.(repoPath, group) : undefined}
                behind={behind[group.stack.number] ?? null}
                onCompare={() => compareBase(group)}
              >
                {#snippet row(entry, index)}
                  {@render prRow(entry, index, group)}
                {/snippet}
              </StackCard>
            {/if}
          {/each}
        {:else}
          <div class="panel-card overflow-hidden">
            <div role="list" class="divide-y divide-surface-200-800">
              {#each section.items as item (item.kind === "stack"
                ? `stack-${item.group.stack.number}`
                : item.row.pr.number)}
                {#if item.kind === "stack"}
                  {@const group = item.group}
                  <div role="listitem" aria-label="Stack {group.name}" class="contents">
                    <StackCard {group} mine={false}>
                      {#snippet row(entry, index)}
                        {@render prRow(entry, index, group)}
                      {/snippet}
                    </StackCard>
                  </div>
                {:else}
                  {@render prRow(item.row, 0, null)}
                {/if}
              {/each}
            </div>
          </div>
        {/if}
      {:else}
        <div class="empty-state">
          {inbox.prs.length === 0 ? "No open pull requests." : "No pull requests match these filters."}
        </div>
      {/each}
    {:else}
      <div class="space-y-2" aria-label="Loading pull requests">
        <div class="placeholder h-10 animate-pulse rounded-md"></div>
        <div class="placeholder h-10 animate-pulse rounded-md opacity-70"></div>
      </div>
    {/if}

    {#if selected.size > 0}
      <div
        class="popover-card sticky bottom-4 flex flex-wrap items-center gap-1.5 self-center px-2 py-1.5 text-xs"
        role="toolbar"
        aria-label="Selected pull requests"
      >
        <span class="px-1 font-medium">{selected.size} selected</span>
        <button
          type="button"
          class="btn btn-sm text-[11px] preset-tonal"
          disabled={!handlers.startAgentReviews}
          title={handlers.startAgentReviews ? undefined : "Not available yet"}
          onclick={() => handlers.startAgentReviews?.(repoPath, selectedPrs)}
        >
          Start agent reviews
        </button>
        <button
          type="button"
          class="btn btn-sm text-[11px] hover:preset-tonal"
          disabled={!handlers.createWorktrees}
          title={handlers.createWorktrees ? undefined : "Not available yet"}
          onclick={() => handlers.createWorktrees?.(repoPath, selectedPrs)}
        >
          Create worktrees
        </button>
        <button
          type="button"
          class="btn btn-sm text-[11px] hover:preset-tonal"
          onclick={() => {
            void markActivitySeen(
              repoPath,
              selectedPrs.map((pr) => pr.number),
            ).catch(fail);
            clearSelection();
          }}
        >
          Mark seen
        </button>
        <button
          type="button"
          class="btn-icon btn-icon-sm hover:preset-tonal"
          aria-label="Clear selection"
          onclick={clearSelection}
        >
          <X size={14} />
        </button>
      </div>
    {/if}
  </section>

  <!-- Beside the list once both fit (560 + 280px), pinned in view while the
       list scrolls; else under it. -->
  <aside
    aria-label="Since you last looked"
    class="flex min-w-0 flex-[1_1_280px] flex-col gap-3 border-t border-surface-200-800 px-6 py-5 text-xs @min-[840px]/desk:sticky @min-[840px]/desk:top-0 @min-[840px]/desk:h-full @min-[840px]/desk:self-start @min-[840px]/desk:overflow-y-auto @min-[840px]/desk:border-t-0 @min-[840px]/desk:border-l @min-[840px]/desk:px-4"
  >
    {#if inbox}
      <SinceYouLooked
        feed={inbox.feed}
        {now}
        onOpen={(number) => {
          const pr = inbox.prs.find((candidate) => candidate.number === number);
          if (pr) open(pr);
          else void markActivitySeen(repoPath, [number]).catch(fail);
        }}
        onMarkAllSeen={() =>
          void markActivitySeen(repoPath, [...new Set(inbox.feed.map((entry) => entry.number))]).catch(fail)}
      />
      <AgentReviewsPanel reviews={inFlight(agentRuns)} />
    {/if}
  </aside>
</div>
