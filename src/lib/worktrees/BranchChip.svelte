<script lang="ts">
import { GitBranch } from "@lucide/svelte";
import { ensureGitStatus, gitStatusCache, statusIsDirty } from "$lib/worktrees/gitStatus.svelte";

let {
  path,
  onOpen,
  maxWidthClass = "max-w-[9rem]",
}: {
  path: string;
  onOpen: (path: string) => void;
  maxWidthClass?: string;
} = $props();

// Fetch on demand the first time this repo's chip appears; never polls.
$effect(() => {
  ensureGitStatus(path);
});

const status = $derived(gitStatusCache.byPath[path] ?? null);
const dirty = $derived(statusIsDirty(status));
const branch = $derived(status?.detached ? "detached" : (status?.branch ?? null));
</script>

{#if branch}
  <button
    type="button"
    onclick={(event) => {
      event.stopPropagation();
      onOpen(path);
    }}
    title="{branch}{dirty ? ' · uncommitted changes' : ''} — open worktrees"
    class="inline-flex {maxWidthClass} items-center gap-1 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium text-surface-600-400 transition hover:preset-tonal-primary"
  >
    <GitBranch size={9} class="shrink-0" />
    <span class="truncate">{branch}</span>
    {#if dirty}
      <span class="size-1.5 shrink-0 rounded-full bg-warning-500" aria-label="Uncommitted changes"></span>
    {/if}
  </button>
{/if}
