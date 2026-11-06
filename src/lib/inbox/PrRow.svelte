<!--
  One pull request in the inbox: state ring, number and title, review and CI
  badges, unresolved threads, reviewers and one primary action. Row actions
  and shortcut hints (↵ R T) fade in on hover and focus; unseen activity is
  bold with a dot and "N new". In a stack the ring holds the PR's position
  and a rail joins it to the rows above and below. The primary action sits in
  a fixed-width slot, so the badges line up whatever it says. A click opens the PR; ⌘-click or X toggles the
  row's selection and ⇧-click selects a range. The badges sit under the
  title until the inbox is wide enough for one line. Every action goes to
  `onAction` by id.
-->
<script lang="ts">
  import { Ellipsis, MessageSquare, Sparkles, SquarePen } from "@lucide/svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import { RING_CLASS, TONE_CLASS } from "$lib/inbox/badges";
  import {
    type AgentReviewState,
    ciBadge,
    type InboxRow,
    type PrimaryAction,
    reviewBadge,
    reviewers,
    stateTone,
  } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";

  let {
    row,
    primary,
    primaryDisabled = false,
    agentReview = null,
    agentFindings = 0,
    selected = false,
    selecting = false,
    menu,
    ciEntries,
    stack = null,
    onAction,
    onPick,
  }: {
    row: InboxRow;
    primary: PrimaryAction;
    /** The primary action's handler is not wired yet. */
    primaryDisabled?: boolean;
    agentReview?: AgentReviewState | null;
    /** Draft Findings of a finished agent review. */
    agentFindings?: number;
    selected?: boolean;
    /** Some row is selected: rows show their checkboxes. */
    selecting?: boolean;
    menu: MenuEntry[];
    ciEntries: MenuEntry[];
    /** The row's place in its stack card; `first` and `last` are among the card's visible rows. */
    stack?: { position: number; total: number; first: boolean; last: boolean } | null;
    onAction: (id: string) => void;
    onPick: (mode: "toggle" | "range") => void;
  } = $props();

  const pr = $derived(row.pr);
  const triage = $derived(row.triage);
  const review = $derived(reviewBadge(pr));
  const ci = $derived(ciBadge(pr));
  const people = $derived(reviewers(pr));
  const unseen = $derived(triage.newActivity > 0);
  const since = $derived(triage.commitsSinceMyReview ?? 0);
  const agentReviewOff = $derived(
    menu.some((entry) => entry.kind !== "separator" && entry.id === "agentReview" && entry.disabled),
  );

  function isControl(target: EventTarget | null): boolean {
    return target instanceof Element && target.closest("button, a, input, [role=menuitem]") !== null;
  }

  function onclick(event: MouseEvent) {
    if (isControl(event.target)) return;
    if (event.shiftKey) onPick("range");
    else if (event.metaKey || event.ctrlKey || selecting) onPick("toggle");
    else onAction("open");
  }

  function onkeydown(event: KeyboardEvent, menuKeydown: unknown) {
    if (event.target === event.currentTarget && event.key.toLowerCase() === "x" && !event.metaKey && !event.ctrlKey) {
      event.preventDefault();
      onPick("toggle");
      return;
    }
    if (typeof menuKeydown === "function") menuKeydown(event);
  }
</script>

<ActionMenu entries={menu} label="Pull request {pr.number}" onSelect={onAction}>
  {#snippet row({ rowProps, buttonProps })}
    <div
      {...rowProps}
      role="listitem"
      aria-label="#{pr.number} {pr.title}"
      data-unseen={unseen ? "true" : undefined}
      data-selected={selected ? "true" : undefined}
      class="group/pr pr-row relative"
      style:padding-left={selecting ? "36px" : undefined}
      {onclick}
      onkeydown={(event) => onkeydown(event, rowProps.onkeydown)}
    >
      {#if selecting}
        <!-- In a slot of its own at the left edge. -->
        <input
          type="checkbox"
          class="checkbox absolute top-1/2 left-3 size-3.5 -translate-y-1/2"
          aria-label="Select #{pr.number}"
          checked={selected}
          onchange={() => onPick("toggle")}
        >
      {/if}
      {#if stack}
        <!-- Stretched over the row's padding so the rail meets the rows above and below. -->
        <span class="relative -my-2.5 flex items-center self-stretch">
          {#if !stack.first}
            <span
              class="absolute -top-px bottom-[calc(50%+9px)] left-1/2 w-0.5 -translate-x-1/2 bg-surface-300-700"
              aria-hidden="true"
            ></span>
          {/if}
          {#if !stack.last}
            <span
              class="absolute top-[calc(50%+9px)] bottom-0 left-1/2 w-0.5 -translate-x-1/2 bg-surface-300-700"
              aria-hidden="true"
            ></span>
          {/if}
          <span
            class="stack-step {pr.isDraft ? "border-dashed" : ""} {RING_CLASS[stateTone(pr)]}"
            title="{stack.position} of {stack.total} in the stack">{stack.position}</span
          >
        </span>
      {:else if triage.mine}
        <span
          class="size-2.5 shrink-0 rounded-full border-2 {pr.isDraft ? "border-dashed" : ""} {RING_CLASS[
            stateTone(pr)
          ]}"
          aria-hidden="true"
        ></span>
      {:else}
        <LoginAvatar login={pr.author} />
      {/if}
      <!-- Title and badges: two lines in a narrow inbox; one once it is wide (`@container/inbox`), unless the badges would squeeze the title. -->
      <div
        class="flex min-w-0 flex-1 flex-col gap-1 @5xl/inbox:flex-row @5xl/inbox:flex-wrap @5xl/inbox:items-center @5xl/inbox:gap-x-2.5"
      >
        <span class="flex min-w-0 items-center gap-2.5 @5xl/inbox:flex-[1_1_14rem]">
          <span class="font-mono text-[11px] text-surface-500">#{pr.number}</span>
          <span
            class="min-w-0 truncate {unseen ? "font-semibold" : "font-medium"} {pr.isDraft
              ? "text-surface-600-400"
              : ""}"
            >{pr.title}</span
          >
          {#if unseen}
            <span class="unseen-dot" aria-hidden="true"></span>
            <span class="shrink-0 text-surface-500">{triage.newActivity} new</span>
          {/if}
        </span>
        <span class="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1 @5xl/inbox:ml-auto @5xl/inbox:justify-end">
          {#if review}
            <span class="status-badge {TONE_CLASS[review.tone]}">{review.label}</span>
          {/if}
          {#if ci?.tone === "error"}
            <ActionMenu
              entries={ciEntries}
              label="CI failing on #{pr.number}"
              onSelect={onAction}
              buttonLabel="CI failing"
            >
              {#snippet row({ buttonProps: ciButton })}
                <button {...ciButton} type="button" class="status-badge {TONE_CLASS.error}">{ci.label}</button>
              {/snippet}
            </ActionMenu>
          {:else if ci}
            <span class="status-badge {TONE_CLASS[ci.tone]}">{ci.label}</span>
          {/if}
          {#if pr.unresolvedThreads > 0}
            <span class="inline-flex items-center gap-1">
              <MessageSquare size={12} aria-hidden="true" />{pr.unresolvedThreads}
              unresolved
            </span>
          {/if}
          {#if since > 0}
            <span class="status-badge {TONE_CLASS.warning}"
              >{since}
              commit{since === 1 ? "" : "s"}
              since your {triage.myReview === "APPROVED" ? "approval" : "review"}</span
            >
          {/if}
          {#if agentReview === "ready"}
            <span class="status-badge {TONE_CLASS.primary}">
              <Sparkles size={10} aria-hidden="true" />Agent review ready · {agentFindings}
              finding{agentFindings === 1 ? "" : "s"}
            </span>
          {:else if agentReview}
            <span class="status-badge {agentReview === "running" ? TONE_CLASS.primary : TONE_CLASS.surface}">
              <Sparkles size={10} aria-hidden="true" />Agent review {agentReview}
            </span>
          {/if}
          {#if people.length > 0}
            <span class="inline-flex">
              {#each people as login, index (login)}
                <LoginAvatar {login} class={index > 0 ? "-ml-1" : ""} />
              {/each}
            </span>
          {:else if triage.mine}
            <span class="text-surface-500">No reviews</span>
          {/if}
        </span>
      </div>
      <span class="flex shrink-0 items-center gap-2.5">
        <span class="flex w-[70px] items-center justify-end gap-0.5">
          <!-- Keyboard focus swaps the hover actions for their shortcuts, in about the same space. -->
          <span class="hidden items-center gap-1 pr-1 group-focus-visible/pr:inline-flex" aria-hidden="true">
            <kbd class="kbd">↵</kbd><kbd class="kbd">R</kbd><kbd class="kbd">T</kbd>
          </span>
          <button
            type="button"
            class="row-action size-[22px] group-hover/pr:opacity-100 group-focus/pr:opacity-100 group-focus-visible/pr:hidden"
            aria-label="New thread"
            title="New thread on this PR (T)"
            onclick={() => onAction("newThread")}
          >
            <SquarePen size={12} />
          </button>
          <button
            type="button"
            class="row-action size-[22px] group-hover/pr:opacity-100 group-focus/pr:opacity-100 group-focus-visible/pr:hidden"
            aria-label="Start agent review"
            title={agentReviewOff ? "Not available yet" : "Start agent review (R)"}
            disabled={agentReviewOff}
            onclick={() => onAction("agentReview")}
          >
            <Sparkles size={12} />
          </button>
          <button
            {...buttonProps}
            type="button"
            class="row-action size-[22px] group-hover/pr:opacity-100 group-focus/pr:opacity-100"
          >
            <Ellipsis size={13} />
          </button>
        </span>
        <span class="flex w-36 justify-end">
          <button
            type="button"
            class="btn btn-sm max-w-full min-w-0 text-[11px] {primary.id === "open" || primary.id === "agentReview"
              ? "hover:preset-tonal"
              : "preset-tonal"}"
            disabled={primaryDisabled}
            title={primaryDisabled ? "Not available yet" : primary.label}
            onclick={() => onAction(`primary:${primary.id}`)}
          >
            <span class="truncate">{primary.label}</span>
          </button>
        </span>
      </span>
    </div>
  {/snippet}
</ActionMenu>
