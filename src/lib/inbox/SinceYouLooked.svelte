<!-- "Since you last looked": activity on the repository's PRs not yet
     marked seen, newest first. Choosing an entry opens its PR. -->
<script lang="ts">
  import { ago, feedText } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";
  import type { FeedEntry } from "$lib/types";

  let {
    feed,
    now,
    onOpen,
    onMarkAllSeen,
  }: {
    feed: FeedEntry[];
    now: number;
    onOpen: (number: number) => void;
    onMarkAllSeen: () => void;
  } = $props();
</script>

<div class="flex items-center gap-2">
  <h2 class="eyebrow grow">Since you last looked</h2>
  <button
    type="button"
    class="btn btn-sm text-[11px] hover:preset-tonal"
    disabled={feed.length === 0}
    onclick={onMarkAllSeen}
  >
    Mark all seen
  </button>
</div>
{#if feed.length === 0}
  <p class="px-2 text-xs text-surface-500">Nothing new since you last looked.</p>
{:else}
  <div class="flex flex-col gap-0.5">
    {#each feed as entry (entry.id)}
      {@const line = feedText(entry)}
      <button type="button" class="menu-row items-start p-2" onclick={() => onOpen(entry.number)}>
        {#if line.actor}
          <LoginAvatar login={line.actor} />
        {:else}
          <span
            class="grid size-[18px] shrink-0 place-items-center rounded-full text-[9px] font-semibold {entry.kind ===
            "checksFailed"
              ? "preset-tonal-error"
              : "preset-tonal-primary"}"
            aria-hidden="true"
            >{entry.kind === "checksFailed" ? "!" : "#"}</span
          >
        {/if}
        <span class="flex grow flex-col gap-0.5">
          <span
            >{#if line.actor}
              <b class="font-medium">{line.actor}</b>
            {/if}
            {line.text}</span
          >
          <span class="text-surface-500">{ago(entry.at, now)}</span>
        </span>
        <span class="unseen-dot mt-[5px]" aria-hidden="true"></span>
      </button>
    {/each}
  </div>
{/if}
