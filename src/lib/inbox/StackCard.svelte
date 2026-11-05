<!--
  A stack as one card: its name, base and PRs bottom first, each indented
  one step above the PR it builds on. My stacks show their worktree and
  thread count, with Restack and Open stack when the Stack page is wired;
  another author's stack shows who wrote it.
-->
<script lang="ts">
  import { Folder, GitBranch } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import type { InboxRow, StackGroup } from "$lib/inbox/inbox";
  import type { WorktreeEntry } from "$lib/types";

  let {
    group,
    mine,
    worktree = null,
    threadCount = 0,
    onOpenStack,
    onRestack,
    row,
  }: {
    group: StackGroup;
    mine: boolean;
    /** The worktree one of the stack's git branches is checked out in. */
    worktree?: WorktreeEntry | null;
    threadCount?: number;
    onOpenStack?: () => void;
    onRestack?: () => void;
    row: Snippet<[InboxRow, number]>;
  } = $props();

  const count = $derived(group.stack.prs.filter((entry) => entry.state === "open").length);
  const worktreeName = $derived(worktree?.path.split("/").pop() ?? "");
</script>

<section class="panel-card overflow-hidden" aria-label="Stack {group.name}">
  <div
    class="flex flex-wrap items-center gap-2 border-b border-surface-200-800 px-3 py-2.5 text-xs {mine
      ? ""
      : "bg-surface-50-950"}"
  >
    <GitBranch size={mine ? 14 : 12} class="text-surface-500" aria-hidden="true" />
    <span class="font-semibold">{group.name}</span>
    {#if mine}
      <span class="text-surface-500">{count} PR{count === 1 ? "" : "s"} onto</span>
      <span class="status-badge preset-tonal font-mono">{group.stack.baseRef}</span>
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
