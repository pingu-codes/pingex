<!--
  A stack as one card: its name, base and PRs bottom first, flush left; the
  order alone shows which PR builds on which. My stacks show their worktree and
  thread count, with Restack and Open stack when the Stack page is wired;
  another author's stack shows who wrote it, and sits in its section's list
  without a card of its own.
-->
<script lang="ts">
  import { Folder, GitBranch } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { InboxRow, StackGroup } from "$lib/inbox/inbox";
  import { behindLabel } from "$lib/stack/stack";
  import type { WorktreeEntry } from "$lib/types";

  let {
    group,
    mine,
    worktree = null,
    threadCount = 0,
    onOpenStack,
    onRestack,
    behind = null,
    onCompare,
    row,
  }: {
    group: StackGroup;
    mine: boolean;
    /** The worktree one of the stack's git branches is checked out in. */
    worktree?: WorktreeEntry | null;
    threadCount?: number;
    onOpenStack?: () => void;
    onRestack?: () => void;
    /** Commits the base has that the stack doesn't; `null` when unknown. */
    behind?: number | null;
    /** What the base gained since the stack forked. */
    onCompare?: () => void;
    row: Snippet<[InboxRow, number]>;
  } = $props();

  const count = $derived(group.stack.prs.filter((entry) => entry.state === "open").length);
  const worktreeName = $derived(worktree?.path.split("/").pop() ?? "");
  const behindText = $derived(behindLabel(group.stack.baseRef, behind));
</script>

<section class={mine ? "panel-card overflow-hidden" : ""} aria-label="Stack {group.name}">
  <div class="flex flex-wrap items-center gap-2 border-b border-surface-200-800 px-3 py-2.5 text-xs">
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
      {#if worktree}
        <span class="status-badge preset-tonal" title="Worktree for this stack">
          <Folder size={9} aria-hidden="true" />{worktreeName}
          · {threadCount} thread{threadCount === 1 ? "" : "s"}
        </span>
      {/if}
      {#if onRestack}
        <button type="button" class="btn btn-sm text-xs hover:preset-tonal" onclick={onRestack}>Restack</button>
      {/if}
      {#if onOpenStack}
        <button type="button" class="btn btn-sm text-xs preset-tonal" onclick={onOpenStack}>Open stack</button>
      {/if}
    {:else}
      <span class="text-surface-500">{group.author} · {count} PR{count === 1 ? "" : "s"}</span>
    {/if}
  </div>
  <div role="list" class="divide-y divide-surface-200-800">
    {#each group.rows as entry, index (entry.pr.number)}
      {@render row(entry, index)}
    {/each}
  </div>
</section>
