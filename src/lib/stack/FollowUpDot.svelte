<!-- The sidebar's dot on a worktree whose stack has a merged PR to follow
     up: restack the rest and delete the merged git branch. -->
<script lang="ts">
  import { followUpPaths } from "$lib/stack/followUps.svelte";
  import { trimSlash } from "$lib/utils/path";

  let { path }: { path: string } = $props();

  const due = $derived(followUpPaths().has(trimSlash(path)) || followUpPaths().has(path));
</script>

{#if due}
  <span
    class="unseen-dot bg-success-500"
    role="img"
    aria-label="A PR in this worktree's stack merged"
    title="A PR in this stack merged: restack the rest and clean up"
  ></span>
{/if}
