<!--
  The Changes page's segment rail: each repository the thread works in, and
  under it its Change segments (stack PRs, git branches, the working tree)
  with what each is compared with and how much it changes. Choosing one shows
  its files.
-->
<script lang="ts">
  import {
    CircleAlert,
    FilePen,
    GitBranch,
    GitBranchPlus,
    GitPullRequest,
    Layers,
    Trash2,
    TriangleAlert,
  } from "@lucide/svelte";
  import { segmentKey } from "$lib/changes/changesPage";
  import type { ChangeSegment, RepoChanges } from "$lib/types";

  let {
    repos,
    selectedKey,
    onSelect,
  }: {
    repos: RepoChanges[];
    selectedKey: string | null;
    onSelect: (key: string) => void;
  } = $props();

  function iconFor(segment: ChangeSegment) {
    switch (segment.kind) {
      case "stackPr":
        return GitPullRequest;
      case "uncommitted":
        return FilePen;
      case "newBranch":
        return GitBranchPlus;
      case "deletedBranch":
        return Trash2;
      default:
        return GitBranch;
    }
  }
</script>

<nav
  aria-label="Change segments"
  class="sticky top-0 flex max-h-[100cqh] w-56 shrink-0 flex-col gap-3 self-start overflow-y-auto border-r border-surface-200-800 px-2 py-3"
>
  {#each repos as repo (repo.root)}
    <section aria-label={repo.label} class="space-y-1">
      <header class="flex items-center gap-1.5 px-1.5" title={repo.root}>
        {#if repo.purpose?.kind === "stack"}
          <Layers size={12} class="shrink-0 text-primary-500" aria-label="Stack worktree" />
        {/if}
        <span class="min-w-0 flex-1 truncate eyebrow text-[10px]">{repo.label}</span>
        {#if repo.branch}
          <span class="max-w-28 truncate font-mono text-[10px] text-surface-500" title={repo.branch}
            >{repo.branch}</span
          >
        {/if}
      </header>
      {#if repo.error}
        <p class="flex gap-1.5 px-1.5 text-[11px] text-error-600-400">
          <CircleAlert size={12} class="mt-0.5 shrink-0" aria-hidden="true" />{repo.error}
        </p>
      {:else if repo.segments.length === 0}
        <p class="px-1.5 text-[11px] text-surface-500">{repo.note ?? "No changes."}</p>
      {/if}
      {#if repo.inProgress}
        <p class="flex gap-1.5 px-1.5 text-[11px] text-warning-700-300">
          <TriangleAlert size={12} class="mt-0.5 shrink-0" aria-hidden="true" />A {repo.inProgress} is stopped half way
        </p>
      {/if}
      {#each repo.segments as segment (segment.id)}
        {@const key = segmentKey(repo.root, segment.id)}
        {@const Icon = iconFor(segment)}
        {@const selected = key === selectedKey}
        <button
          type="button"
          aria-current={selected ? "true" : undefined}
          onclick={() => onSelect(key)}
          class="flex w-full flex-col gap-0.5 rounded-md px-1.5 py-1.5 text-left {selected
            ? "preset-tonal-primary"
            : "hover:preset-tonal"}"
        >
          <span class="flex w-full items-center gap-1.5">
            <Icon
              size={13}
              class="shrink-0 {segment.kind === "uncommitted" ? "text-warning-500" : "text-surface-500"}"
            />
            <span class="min-w-0 flex-1 truncate text-xs font-medium" title={segment.label}>{segment.label}</span>
            {#if segment.current && segment.kind !== "uncommitted"}
              <span class="size-1.5 shrink-0 rounded-full bg-success-500" title="Checked out"></span>
            {/if}
          </span>
          <span class="flex w-full items-center gap-1.5 pl-5 text-[10px] text-surface-500">
            <span class="min-w-0 flex-1 truncate" title="Compared with {segment.baseLabel}"
              >vs {segment.baseLabel}</span
            >
            {#if segment.totalFiles > 0}
              <span class="shrink-0 tabular-nums">{segment.totalFiles}</span>
              <span class="shrink-0 tabular-nums text-success-600-400">+{segment.additions}</span>
              <span class="shrink-0 tabular-nums text-error-600-400">−{segment.deletions}</span>
            {/if}
          </span>
          {#if segment.needsRestack}
            <span class="pl-5 text-[10px] text-warning-700-300">Not on top of the PR below</span>
          {:else if segment.rewritten}
            <span class="pl-5 text-[10px] text-warning-700-300">Rewritten since the thread started</span>
          {:else if segment.note}
            <span class="pl-5 text-[10px] text-surface-500">{segment.note}</span>
          {/if}
        </button>
      {/each}
    </section>
  {/each}
</nav>
