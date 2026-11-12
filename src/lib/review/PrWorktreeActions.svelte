<script lang="ts">
  import { CHECK_OUT_FIRST } from "$lib/review/prOnDisk";
  import type { PrCheckout } from "$lib/types";
  /**
   * A PR page's worktree state and what to do with it: Check out until the PR
   * is checked out, then Work on this PR, what the worktree holds that the PR
   * doesn't, and pushing it back. Agents can change things in the worktree;
   * nothing reaches the PR until someone pushes.
   */
  import { plural } from "$lib/utils/format";

  let {
    checkout = null,
    known = true,
    mine,
    onCheckOut,
    onWork,
    onPush,
  }: {
    checkout?: PrCheckout | null;
    /** `checkout` has been read: `null` then means it isn't checked out. */
    known?: boolean;
    mine: boolean;
    onCheckOut?: () => void;
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
{#if known && !checkout && onCheckOut}
  <button type="button" class="btn btn-sm preset-filled-primary-500 text-xs" onclick={onCheckOut}>Check out</button>
{/if}
{#if onWork}
  <button
    type="button"
    class="btn btn-sm text-xs {mine && checkout ? "preset-filled-primary-500" : "hover:preset-tonal"}"
    title={checkout?.path ?? CHECK_OUT_FIRST}
    disabled={!checkout}
    onclick={onWork}
  >
    Work on this PR
  </button>
{/if}
