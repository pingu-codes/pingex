<script lang="ts">
  import { MessageCircleQuestion, Trash2 } from "@lucide/svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import type { SideQuestion } from "$lib/types";

  // A thread's side questions, each opening on click with a hover-revealed delete.
  let {
    entries,
    onOpen,
    onDelete,
  }: {
    entries: SideQuestion[];
    onOpen: (entry: SideQuestion) => void;
    onDelete: (entry: SideQuestion) => void;
  } = $props();
</script>

<div class="space-y-1">
  {#each entries as entry (entry.sideThreadId)}
    <div class="group/side relative">
      <button
        onclick={() => onOpen(entry)}
        class="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 pr-8 text-left text-xs hover:preset-tonal"
      >
        <MessageCircleQuestion size={13} class="shrink-0 text-primary-500" />
        <span class="min-w-0 flex-1 truncate">{entry.title}</span>
      </button>
      <TooltipButton
        label="Delete side question"
        aria-label={`Delete side question ${entry.title}`}
        onclick={() => onDelete(entry)}
        class="absolute right-1.5 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-300-700 hover:text-error-500 focus:opacity-100 group-hover/side:opacity-100"
      >
        <Trash2 size={12} />
      </TooltipButton>
    </div>
  {/each}
</div>
