<!--
  One pull request in the inbox: state ring, number and title, review and CI
  badges, unresolved threads, reviewers and one primary action. Row actions
  and shortcut hints (↵ R T) fade in on hover and focus; unseen activity is
  bold with a dot and "N new". ⌘-click or X toggles the row's selection and
  ⇧-click selects a range. Every action goes to `onAction` by id.
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
    depth = 0,
    primary,
    primaryDisabled = false,
    agentReview = null,
    selected = false,
    selecting = false,
    menu,
    ciEntries,
    onAction,
    onPick,
  }: {
    row: InboxRow;
    /** Indent inside a stack card: 0 for the bottom PR. */
    depth?: number;
    primary: PrimaryAction;
    /** The primary action's handler is not wired yet. */
    primaryDisabled?: boolean;
    agentReview?: AgentReviewState | null;
    selected?: boolean;
    /** Some row is selected: rows show their checkboxes. */
    selecting?: boolean;
    menu: MenuEntry[];
    ciEntries: MenuEntry[];
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
      class="group/pr pr-row"
      style:padding-left={depth > 0 ? `${12 + depth * 20}px` : undefined}
      {onclick}
      onkeydown={(event) => onkeydown(event, rowProps.onkeydown)}
    >
      {#if selecting}
        <input
          type="checkbox"
          class="checkbox size-3.5"
          aria-label="Select #{pr.number}"
          checked={selected}
          onchange={() => onPick("toggle")}
        >
      {/if}
      {#if depth > 0}
        <span class="-mt-2.5 -ml-4 size-3 shrink-0 border-b border-l border-surface-400-600" aria-hidden="true"></span>
      {/if}
      {#if triage.mine}
        <span
          class="size-2.5 shrink-0 rounded-full border-2 {pr.isDraft ? "border-dashed" : ""} {RING_CLASS[
            stateTone(pr)
          ]}"
          aria-hidden="true"
        ></span>
      {:else}
        <LoginAvatar login={pr.author} />
      {/if}
      <span class="font-mono text-[11px] text-surface-500">#{pr.number}</span>
      <span
        class="min-w-0 flex-[1_1_220px] truncate {unseen ? "font-semibold" : "font-medium"} {pr.isDraft
          ? "text-surface-600-400"
          : ""}"
        >{pr.title}</span
      >
      {#if unseen}
        <span class="unseen-dot" aria-hidden="true"></span>
        <span class="text-surface-500">{triage.newActivity} new</span>
      {/if}
      {#if review}
        <span class="status-badge {TONE_CLASS[review.tone]}">{review.label}</span>
      {/if}
      {#if ci?.tone === "error"}
        <ActionMenu entries={ciEntries} label="CI failing on #{pr.number}" onSelect={onAction} buttonLabel="CI failing">
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
      {#if agentReview}
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
      <span class="hidden items-center gap-1 group-focus/pr:inline-flex" aria-hidden="true">
        <kbd class="kbd">↵</kbd><kbd class="kbd">R</kbd><kbd class="kbd">T</kbd>
      </span>
      <span class="flex items-center gap-0.5">
        <button
          type="button"
          class="row-action size-[22px] group-hover/pr:opacity-100 group-focus/pr:opacity-100"
          aria-label="New thread"
          title="New thread on this PR (T)"
          onclick={() => onAction("newThread")}
        >
          <SquarePen size={12} />
        </button>
        <button
          type="button"
          class="row-action size-[22px] group-hover/pr:opacity-100 group-focus/pr:opacity-100"
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
      <button
        type="button"
        class="btn btn-sm text-[11px] {primary.id === "open" || primary.id === "agentReview"
          ? "hover:preset-tonal"
          : "preset-tonal"}"
        disabled={primaryDisabled}
        title={primaryDisabled ? "Not available yet" : undefined}
        onclick={() => onAction(`primary:${primary.id}`)}
      >
        {primary.label}
      </button>
    </div>
  {/snippet}
</ActionMenu>
