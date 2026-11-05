<script lang="ts">
  import { ArrowUp, Square } from "@lucide/svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";

  // The side-question input: Enter asks, Shift+Enter breaks the line, and
  // Escape stops an answer in progress.
  let {
    value = $bindable(""),
    followUp,
    busy,
    disabled = false,
    onAsk,
    onStop,
  }: {
    value?: string;
    /** A side question is open, so the next message follows up on it. */
    followUp: boolean;
    /** An answer is in progress: Stop shows and asking waits. */
    busy: boolean;
    /** Asking is blocked for another reason, such as a load in flight. */
    disabled?: boolean;
    onAsk: () => void;
    onStop: () => void;
  } = $props();
</script>

<div class="border-t border-surface-200-800 p-2">
  <div
    class="flex items-end gap-2 rounded-xl border border-surface-200-800 bg-surface-50-950 px-2.5 py-1.5 focus-within:border-surface-400-600"
  >
    <textarea
      bind:value
      onkeydown={(event) => {
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          onAsk();
        } else if (event.key === "Escape" && busy) {
          event.preventDefault();
          onStop();
        }
      }}
      rows="1"
      placeholder={followUp ? "Follow up…" : "Ask a side question…"}
      class="max-h-24 min-h-[1.25rem] flex-1 resize-none bg-transparent text-xs leading-5 outline-none placeholder:text-surface-500"
    ></textarea>
    {#if busy}
      <TooltipButton
        label="Stop (Esc)"
        onclick={onStop}
        aria-label="Stop side question"
        class="grid size-6 shrink-0 place-items-center rounded-full preset-filled-error-500"
      >
        <Square size={10} fill="currentColor" />
      </TooltipButton>
    {/if}
    <TooltipButton
      label={busy ? "Wait for the answer or press Stop" : "Ask side question"}
      onclick={onAsk}
      aria-label="Ask side question"
      disabled={busy || disabled || !value.trim()}
      class="grid size-6 shrink-0 place-items-center rounded-full preset-filled-primary-500 disabled:opacity-40"
    >
      <ArrowUp size={12} />
    </TooltipButton>
  </div>
</div>
