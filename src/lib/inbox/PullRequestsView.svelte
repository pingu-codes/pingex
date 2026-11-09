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
  import { Search, X } from "@lucide/svelte";
  import { ToggleGroup } from "@skeletonlabs/skeleton-svelte";
  import { onMount } from "svelte";
  import { projects } from "$lib/app/appData.svelte";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import { place } from "$lib/app/places";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import RefreshIcon from "$lib/components/RefreshIcon.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import AccountMenu from "$lib/inbox/AccountMenu.svelte";
  import AgentReviewsPanel from "$lib/inbox/AgentReviewsPanel.svelte";
  import { inFlight, latestPerPr, loadAgentReviews, repoAgentReviews } from "$lib/inbox/agentReviews";
  import CheckLogDialog from "$lib/inbox/CheckLogDialog.svelte";
  import type { InboxHandlers } from "$lib/inbox/handlers";
  import {
    FILTER_LABELS,
    type InboxChips,
    type InboxFilter,
    type InboxRow,
    inboxSections,
    type People,
    peopleByLogin,
    primaryAction,
    rangeSelection,
    type StackGroup,
    sectionOrder,
    stackPosition,
    visibleRows,
  } from "$lib/inbox/inbox";
  import { inboxes, markActivitySeen, mutePr, syncInbox } from "$lib/inbox/inboxStore.svelte";
  import { ciMenu, prMenu } from "$lib/inbox/menus";
  import PrRow from "$lib/inbox/PrRow.svelte";
  import RepoPicker from "$lib/inbox/RepoPicker.svelte";
  import SinceYouLooked from "$lib/inbox/SinceYouLooked.svelte";
  import StackCard from "$lib/inbox/StackCard.svelte";
  import CompareCommitsDialog from "$lib/review/CompareCommitsDialog.svelte";
  import { inboxPrTarget } from "$lib/review/prTarget";
  import { commands } from "$lib/services/homeRouting";
  import { query } from "$lib/services/queryCache.svelte";
  import { driftLabel, myBaseInputs, stackInputFor } from "$lib/stack/stack";
  import { toastFailure } from "$lib/toaster";
  import type { CheckoutTarget, InboxPr, PrBaseInput, PrCheckout } from "$lib/types";
  import { relativeTime } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";
  import { readFlag, writeFlag } from "$lib/utils/storedFlag";
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

  // svelte-ignore state_referenced_locally -- the view is keyed by repository.
  const left = place(`inbox:${repoPath}`, () => ({
    filter: "needsMe" as InboxFilter,
    chips: { drafts: false, ciFailing: false, unresolved: false } as InboxChips,
    text: "",
    selected: new Set<number>(),
    anchor: null as number | null,
    scrollTop: 0,
  }));

  let filter = $state(left.filter);
  let chips = $state(left.chips);
  let text = $state(left.text);
  let groupByStack = $state(readFlag(GROUP_KEY, true));
  // svelte-ignore state_referenced_locally -- a PR that closed while away is no longer selected.
  let selected = $state(
    new Set([...left.selected].filter((number) => inboxes[repoPath]?.inbox?.prs.some((pr) => pr.number === number))),
  );
  let anchor = $state(left.anchor);
  let now = $state(Date.now());
  let scroller = $state<HTMLElement>();

  $effect(() => {
    Object.assign(left, { filter, chips: $state.snapshot(chips), text, selected, anchor });
  });

  const repoState = $derived(inboxes[repoPath]);
  const inbox = $derived(repoState?.inbox ?? null);
  const stacks = $derived(repoState?.stacks ?? { supported: false, stacks: [] });
  const people = $derived<People>(inbox ? peopleByLogin(inbox) : new Map());
  const repoName = $derived(repos.find((repo) => repo.path === repoPath)?.name ?? basename(repoPath));
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
  // How far behind its base, as GitHub has both, each of my open PRs is:
  // "Behind main by 3" on a stack or a single PR, "Behind #12" on a PR that
  // no longer sits on the head of the one below. Cached, so coming back to
  // the inbox does not run the git work again.
  const behindArgs = $derived.by((): [string, PrBaseInput[]] | null => {
    const mine = inbox ? myBaseInputs(inbox) : [];
    return mine.length > 0 ? [repoPath, mine] : null;
  });
  const behindQuery = query("reviewPrsBehind", () => behindArgs);
  const behind = $derived(new Map((behindQuery.value ?? []).map((row) => [row.number, row.behind])));
  // Where each open PR is on disk, for the rows' worktree icons. Over the whole
  // inbox, not the filtered rows, so changing a filter doesn't read again.
  const checkoutArgs = $derived.by((): [string, CheckoutTarget[]] | null => {
    if (!inbox || inbox.prs.length === 0) return null;
    const viewer = inbox.viewer;
    return [repoPath, inbox.prs.map((pr) => inboxPrTarget(repoPath, pr, viewer))];
  });
  const checkoutQuery = query("reviewPrCheckouts", () => checkoutArgs);
  const checkouts = $derived(new Map((checkoutQuery.value ?? []).map((checkout) => [checkout.number, checkout])));
  const checkoutsKnown = $derived(checkoutQuery.value !== undefined);
  /** The stack's worktree, as one of its open PRs has it: the one on its own branch if any. */
  function stackCheckout(group: StackGroup): PrCheckout | null {
    const members = group.stack.prs.flatMap((entry) => checkouts.get(entry.number) ?? []);
    return members.find((checkout) => checkout.onPrBranch) ?? members[0] ?? null;
  }
  /** The stack's lead: how far its bottom open PR is behind the stack base. */
  function stackBehind(group: StackGroup): number | null {
    const open = new Set(
      group.stack.prs.filter((entry) => entry.state === "open" && !entry.merged).map((e) => e.number),
    );
    const bottom = inbox?.prs.find((pr) => open.has(pr.number) && pr.baseRef === group.stack.baseRef);
    return bottom ? (behind.get(bottom.number) ?? null) : null;
  }
  /** A row's own badge. In a stack card the bottom PR's lead is the card's. */
  function rowDrift(pr: InboxPr, group: StackGroup | null): string | null {
    if (group && pr.baseRef === group.stack.baseRef) return null;
    return driftLabel(pr, behind.get(pr.number), inbox);
  }

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
    await loadAgentReviews(repoPath);
  }

  // "Synced 3m ago" and the feed's times: after each sync, and as time passes.
  $effect(() => {
    void inbox?.syncedAt;
    now = Date.now();
    const timer = setInterval(() => (now = Date.now()), 60_000);
    return () => clearInterval(timer);
  });

  onMount(() => {
    ensureWorktrees([repoPath]);
    if (!inboxes[repoPath]?.inbox) void sync();
    else {
      if (scroller) scroller.scrollTop = left.scrollTop;
      void loadAgentReviews(repoPath);
    }
  });

  function setGroupByStack(on: boolean) {
    groupByStack = on;
    writeFlag(GROUP_KEY, on);
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

  function open(pr: InboxPr, options: { changesSince?: boolean } = {}) {
    void markActivitySeen(repoPath, [pr.number]).catch(toastFailure);
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
      toastFailure(cause);
    }
  }

  async function viewLog(pr: InboxPr) {
    try {
      const logs = await commands.reviewCheckLogs(repoPath, pr.number);
      await openDialog(CheckLogDialog, { title: `Failing checks on #${pr.number}`, logs });
    } catch (cause) {
      toastFailure(cause);
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
        return void commands.copyText(pr.headRef).catch(toastFailure);
      case "copyLink":
        return void commands.copyText(pr.url).catch(toastFailure);
      case "openGitHub":
        return void commands.openExternalUrl(pr.url).catch(toastFailure);
      case "seen":
        return void markActivitySeen(repoPath, [pr.number]).catch(toastFailure);
      case "mute":
        return void mutePr(repoPath, pr.number, pr.headSha).catch(toastFailure);
      case "rerunChecks":
        return void commands.reviewRerunFailedChecks(repoPath, pr.number).then(sync).catch(toastFailure);
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

  const syncedAgo = $derived(
    inbox ? relativeTime(inbox.syncedAt / 1000, { granularity: "minute", suffix: " ago", now: now / 1000 }) : "",
  );
</script>

{#snippet prRow(row: InboxRow, depth: number, group: StackGroup | null)}
  {@const action = primaryAction(row, {
    stackAbove: group !== null && stackPosition(group, row.pr.number).position > 1 && !row.triage.mine,
    agentReview: agentState(row.pr.number),
  })}
  <PrRow
    {row}
    {people}
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
    stack={group
      ? { ...stackPosition(group, row.pr.number), first: depth === 0, last: depth === group.rows.length - 1 }
      : null}
    drift={rowDrift(row.pr, group)}
    checkout={checkouts.get(row.pr.number) ?? null}
    checkoutKnown={checkoutsKnown}
    onAction={(id) => run(id, row, group)}
    onPick={(mode) => pick(row.pr.number, mode)}
  />
{/snippet}

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  bind:this={scroller}
  class="@container/desk flex h-full min-h-0 flex-wrap content-start overflow-y-auto"
  role="region"
  aria-label="Pull requests"
  onkeydown={onKeydown}
  onscroll={(event) => (left.scrollTop = event.currentTarget.scrollTop)}
>
  <section class="@container/inbox flex min-w-0 flex-[999_1_560px] flex-col gap-4 px-6 py-5">
    <header class="flex flex-wrap items-center gap-3">
      <div class="min-w-[200px] grow basis-[200px]">
        <!-- With several repositories the heading is the picker. -->
        {#if repos.length > 1}
          <h1 class="sr-only">{repoName}</h1>
          <div class="text-base"><RepoPicker {repos} {repoPath} {onSelectRepo} /></div>
        {:else}
          <h1 class="w-fit text-base font-semibold">{repoName}</h1>
        {/if}
        <div class="flex flex-wrap items-center gap-x-1.5 text-xs text-surface-500">
          {#if inbox}
            <span>{inbox.repo} · synced {syncedAgo}</span>
          {:else if repoState?.syncing}
            <span>Syncing…</span>
          {/if}
          <!-- Outside the inbox: a pin to an account that has gone is why there may not be one. -->
          <AccountMenu
            {repoPath}
            viewer={inbox?.viewer ?? null}
            syncing={repoState?.syncing ?? false}
            onError={toastFailure}
          />
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
        <RefreshIcon spinning={repoState?.syncing} size={14} />Sync
      </button>
    </header>

    {#if repoState?.error && !inbox}
      <div class="error-note">{repoState.error}</div>
    {:else if inbox}
      <div class="flex flex-wrap items-center gap-2">
        <Segmented label="Show" value={filter} options={filterOptions} onChange={(value) => (filter = value)} />
        <span class="grow"></span>
        <ToggleGroup
          multiple
          value={CHIPS.filter((chip) => chips[chip.key]).map((chip) => chip.key)}
          onValueChange={(details) => {
            chips = {
              drafts: details.value.includes("drafts"),
              ciFailing: details.value.includes("ciFailing"),
              unresolved: details.value.includes("unresolved"),
            };
          }}
          aria-label="Narrow the list"
          class="flex gap-1.5 overflow-visible rounded-none border-0 bg-transparent p-0"
        >
          {#each CHIPS as chip (chip.key)}
            <ToggleGroup.Item value={chip.key} class="pill data-[state=on]:pill-active">{chip.label}</ToggleGroup.Item>
          {/each}
        </ToggleGroup>
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
              {@const checkout = stackCheckout(group)}
              <StackCard
                {group}
                {people}
                mine={true}
                {checkout}
                checkoutKnown={checkoutsKnown}
                threadCount={checkout?.path ? threadCountForPath(projects(), checkout.path) : 0}
                onCheckOut={handlers.checkOutStack ? () => handlers.checkOutStack?.(repoPath, group) : undefined}
                onOpenStack={handlers.openStack ? () => handlers.openStack?.(repoPath, group) : undefined}
                onRestack={handlers.restack ? () => handlers.restack?.(repoPath, group) : undefined}
                behind={stackBehind(group)}
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
                    <StackCard
                      {group}
                      {people}
                      mine={false}
                      checkout={stackCheckout(group)}
                      checkoutKnown={checkoutsKnown}
                      onCheckOut={handlers.checkOutStack ? () => handlers.checkOutStack?.(repoPath, group) : undefined}
                    >
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

    {#if selectedPrs.length > 0}
      <div
        class="popover-card sticky bottom-4 flex flex-wrap items-center gap-1.5 self-center px-2 py-1.5 text-xs"
        role="toolbar"
        aria-label="Selected pull requests"
      >
        <span class="px-1 font-medium">{selectedPrs.length} selected</span>
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
            ).catch(toastFailure);
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
        {people}
        {now}
        onOpen={(number) => {
          const pr = inbox.prs.find((candidate) => candidate.number === number);
          if (pr) open(pr);
          else void markActivitySeen(repoPath, [number]).catch(toastFailure);
        }}
        onMarkAllSeen={() =>
          void markActivitySeen(repoPath, [...new Set(inbox.feed.map((entry) => entry.number))]).catch(toastFailure)}
      />
      <AgentReviewsPanel reviews={inFlight(agentRuns)} />
    {/if}
  </aside>
</div>
