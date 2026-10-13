<script lang="ts">
import { Pencil, Send, X } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { InlineEdit } from "$lib/thread/inlineEdit.svelte";
import { editedParts, messageText } from "$lib/thread/messageText";
import { isLocalOnly, queuedText } from "$lib/thread/queueEntries";
import type { QueuedSubmission, UserInputPart } from "$lib/types";

let {
  entry,
  canSendNow,
  onSendNow,
  onEdit,
  onCancel,
}: {
  entry: QueuedSubmission;
  /** Whether "send now" makes sense — a turn is running or this isn't the head. */
  canSendNow: boolean;
  onSendNow: () => void;
  onEdit: (input: UserInputPart[]) => void;
  onCancel: () => void;
} = $props();

// Only the prose is edited; chips keep their place, as in a sent message's edit.
const edit = new InlineEdit(
  () => messageText(entry.input),
  (text) => onEdit(editedParts(entry.input, text)),
);
</script>

<div
  class="flex items-start gap-2 rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-1.5 text-xs text-surface-600-400"
>
  <span
    class="shrink-0 pt-0.5 font-medium text-surface-500"
    title={isLocalOnly(entry) ? "Held in this window only — this Codex version can't save queued messages." : undefined}
  >
    {isLocalOnly(entry) ? "Queued locally" : "Queued"}
  </span>
  {#if edit.editing}
    <textarea
      bind:this={edit.textarea}
      bind:value={edit.draft}
      onkeydown={edit.onkeydown}
      onblur={edit.submit}
      aria-label="Edit queued message"
      rows="2"
      class="min-w-0 flex-1 resize-none bg-transparent text-surface-800-200 outline-none"
    ></textarea>
  {:else}
    <span class="min-w-0 flex-1 truncate pt-0.5">{queuedText(entry.input)}</span>
    <div class="flex shrink-0 items-center gap-2.5">
      {#if canSendNow}
        <TooltipButton
          label="Send now (stops the current turn)"
          aria-label="Send now"
          onclick={onSendNow}
          class="btn-icon btn-icon-sm rounded-md text-primary-500 transition hover:preset-tonal-primary"
        >
          <Send size={14} />
        </TooltipButton>
      {/if}
      <TooltipButton
        label="Edit queued message"
        onclick={edit.start}
        class="btn-icon btn-icon-sm rounded-md text-surface-600-400 transition hover:preset-tonal"
      >
        <Pencil size={14} />
      </TooltipButton>
      <TooltipButton
        label="Remove queued message"
        onclick={onCancel}
        class="btn-icon btn-icon-sm rounded-md text-surface-600-400 transition hover:text-error-500 hover:preset-tonal"
      >
        <X size={14} />
      </TooltipButton>
    </div>
  {/if}
</div>
