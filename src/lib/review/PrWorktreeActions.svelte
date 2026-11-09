<script lang="ts">
  import type { PrCheckout } from "$lib/types";
  /**
   * A PR page's worktree state and what to do with it: Work on this PR, what the
   * worktree holds that the PR doesn't, and pushing it back. Below it all sits the
   * point of a worktree: agents can change things here, and nothing reaches the
   * PR until someone pushes.
   */
  import { plural } from "$lib/utils/format";

  let {
    checkout = null,
    mine,
    onWork,
    onPush,
  }: {
    checkout?: PrCheckout | null;
    mine: boolean;
    onWork?: () => void;
    onPush?: () => void;
  } = $props();

  const ahead = $derived(checkout?.ahead ?? 0);
  const behind = $derived(checkout?.behind ?? 0);
</script>

{#if ahead > 0}
  <span class="status-badge preset-tonal">{plural(ahead, "local commit")}</span>
{/if}
{#if checkout?.dirty}
  <span class="status-badge preset-tonal">Uncommitted changes</span>
{/if}
{#if behind > 0}
  <span class="status-badge preset-tonal">Behind PR by {behind}</span>
{/if}
{#if ahead > 0 && onPush}
  <button type="button" class="btn btn-sm preset-tonal text-xs" onclick={onPush}>Push to PR</button>
{/if}
{#if onWork}
  <button
    type="button"
    class="btn btn-sm text-xs {mine ? "preset-filled-primary-500" : "hover:preset-tonal"}"
    title={checkout?.path ?? "Creates a worktree"}
    onclick={onWork}
  >
    Work on this PR
  </button>
{/if}
