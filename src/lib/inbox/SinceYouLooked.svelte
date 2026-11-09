<!-- "Since you last looked": activity on the repository's PRs not yet
     marked seen, newest first. Choosing an entry opens its PR. The heading
     collapses the list; the choice is remembered per viewer. -->
<script lang="ts">
  import { ChevronDown, ChevronRight } from "@lucide/svelte";
  import { feedText, type People } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";
  import type { FeedEntry } from "$lib/types";
  import { relativeTime } from "$lib/utils/format";
  import { readFlag, writeFlag } from "$lib/utils/storedFlag";

  let {
    feed,
    people = new Map(),
    now,
    onOpen,
    onMarkAllSeen,
  }: {
    feed: FeedEntry[];
    people?: People;
    now: number;
    onOpen: (number: number) => void;
    onMarkAllSeen: () => void;
  } = $props();

  const OPEN_KEY = "pingex.inbox.sinceYouLookedOpen";

  const uid = $props.id();
  let open = $state(readFlag(OPEN_KEY, true));

  function toggle() {
    open = !open;
    writeFlag(OPEN_KEY, open);
  }
</script>

<div class="flex items-center gap-2">
  <h2 class="eyebrow grow">
    <button
      type="button"
      class="flex items-center gap-1 text-left uppercase hover:text-surface-950-50"
      aria-expanded={open}
      aria-controls="{uid}-feed"
      onclick={toggle}
    >
      {#if open}
        <ChevronDown size={12} aria-hidden="true" />
      {:else}
        <ChevronRight size={12} aria-hidden="true" />
      {/if}
      Since you last looked
      {#if !open && feed.length > 0}
        <span class="status-badge preset-tonal-primary normal-case">{feed.length}</span>
      {/if}
    </button>
  </h2>
  <button
    type="button"
    class="btn btn-sm text-[11px] hover:preset-tonal"
    disabled={feed.length === 0}
    onclick={onMarkAllSeen}
  >
    Mark all seen
  </button>
</div>
{#if !open}
  <!-- Collapsed: the heading's count stands in for the list. -->
{:else if feed.length === 0}
  <p id="{uid}-feed" class="px-2 text-xs text-surface-500">Nothing new since you last looked.</p>
{:else}
  <div id="{uid}-feed" class="flex flex-col gap-0.5">
    {#each feed as entry (entry.id)}
      {@const line = feedText(entry)}
      <button type="button" class="menu-row items-start p-2" onclick={() => onOpen(entry.number)}>
        {#if line.actor}
          <LoginAvatar login={line.actor} person={people.get(line.actor)} />
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
          <span class="text-surface-500"
            >{relativeTime(entry.at / 1000, { granularity: "minute", now: now / 1000 })}</span
          >
        </span>
        <span class="unseen-dot mt-[5px]" aria-hidden="true"></span>
      </button>
    {/each}
  </div>
{/if}
