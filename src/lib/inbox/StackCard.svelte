<!--
  A stack as one card: its name, base and PRs bottom first, numbered from the
  base and joined by a rail, so it is plain which PR builds on which. Every
  stack says where it is on disk, with Check out stack when it isn't. My stacks
  add their thread count and Restack, and a click on the header opens the Stack
  page when it is wired; another author's stack shows who wrote it, and sits in
  its section's list without a card of its own.
-->
<script lang="ts">
  import { GitBranch } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { InboxRow, People, StackGroup } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";
  import WorktreeBadge from "$lib/review/WorktreeBadge.svelte";
  import { behindLabel } from "$lib/stack/stack";
  import type { PrCheckout } from "$lib/types";

  let {
    group,
    people = new Map(),
    mine,
    checkout = null,
    checkoutKnown = false,
    threadCount = 0,
    onOpenStack,
    onRestack,
    onCheckOut,
    behind = null,
    onCompare,
    row,
  }: {
    group: StackGroup;
    people?: People;
    mine: boolean;
    /** Where the stack is on disk: one of its PRs, at the stack's worktree. */
    checkout?: PrCheckout | null;
    /** `checkout` has been read: `null` then means no worktree. */
    checkoutKnown?: boolean;
    threadCount?: number;
    onOpenStack?: () => void;
    onRestack?: () => void;
    /** A worktree for the stack, and a local git branch for each PR. */
    onCheckOut?: () => void;
    /** Commits the base has that the stack doesn't; `null` when unknown. */
    behind?: number | null;
    /** What the base gained since the stack forked. */
    onCompare?: () => void;
    row: Snippet<[InboxRow, number]>;
  } = $props();

  const count = $derived(group.stack.prs.filter((entry) => entry.state === "open").length);
  const behindText = $derived(behindLabel(group.stack.baseRef, behind));
  const opens = $derived(mine && !!onOpenStack);

  function isControl(target: EventTarget | null): boolean {
    return target instanceof Element && target.closest("button, a, input, [role=menuitem]") !== null;
  }

  function onclick(event: MouseEvent) {
    if (opens && !isControl(event.target)) onOpenStack?.();
  }

  function onkeydown(event: KeyboardEvent) {
    if (!opens || event.target !== event.currentTarget) return;
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    onOpenStack?.();
  }
</script>

{#snippet onDisk()}
  <WorktreeBadge variant="chip" {checkout} known={checkoutKnown} noneHint="Check out stack creates one." />
  {#if checkoutKnown && !checkout && onCheckOut}
    <button type="button" class="btn btn-sm text-xs hover:preset-tonal" onclick={onCheckOut}>Check out stack</button>
  {/if}
{/snippet}

<section class={mine ? "panel-card overflow-hidden" : ""} aria-label="Stack {group.name}">
  <!-- The role is dynamic, so the a11y checks cannot see it is a button. -->
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_no_noninteractive_tabindex -->
  <div
    class={[
      "flex flex-wrap items-center gap-2 border-b border-surface-200-800 px-3 py-2.5 text-xs",
      opens &&
        "cursor-pointer outline-none hover:bg-surface-500/10 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary-500",
    ]}
    role={opens ? "button" : undefined}
    tabindex={opens ? 0 : undefined}
    aria-label={opens ? `Open stack ${group.name}` : undefined}
    {onclick}
    {onkeydown}
  >
    <GitBranch size={mine ? 14 : 12} class="text-surface-500" aria-hidden="true" />
    <span class="font-semibold">{group.name}</span>
    {#if mine}
      <span class="text-surface-500">{count} PR{count === 1 ? "" : "s"} onto</span>
      <span class="status-badge preset-tonal font-mono">{group.stack.baseRef}</span>
      {#if behindText}
        <ActionMenu
          entries={[
            {
              id: "restack",
              label: "Restack",
              disabled: !onRestack,
              title: onRestack ? undefined : "Not available yet",
            },
            { id: "compare", label: "Compare", disabled: !onCompare },
          ]}
          label={behindText}
          buttonLabel={behindText}
          onSelect={(id) => (id === "restack" ? onRestack?.() : onCompare?.())}
        >
          {#snippet row({ buttonProps })}
            <button {...buttonProps} type="button" class="status-badge preset-tonal-warning">{behindText}</button>
          {/snippet}
        </ActionMenu>
      {/if}
      <span class="grow"></span>
      {@render onDisk()}
      {#if checkout}
        <span class="text-surface-500">{threadCount} thread{threadCount === 1 ? "" : "s"}</span>
      {/if}
      {#if onRestack}
        <button type="button" class="btn btn-sm text-xs hover:preset-tonal" onclick={onRestack}>Restack</button>
      {/if}
    {:else}
      <LoginAvatar login={group.author} person={people.get(group.author)} role="Author" />
      <span class="text-surface-500">{group.author} · {count} PR{count === 1 ? "" : "s"}</span>
      <span class="grow"></span>
      {@render onDisk()}
    {/if}
  </div>
  <div role="list" class="divide-y divide-surface-200-800">
    {#each group.rows as entry, index (entry.pr.number)}
      {@render row(entry, index)}
    {/each}
  </div>
</section>
