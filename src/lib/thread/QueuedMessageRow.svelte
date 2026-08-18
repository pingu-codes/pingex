<script lang="ts">
import { Pencil, Send, X } from "@lucide/svelte";
import { tick } from "svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { isLocalOnly } from "$lib/thread/queueEntries";
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

/** The message's prose — chips flatten to their `@name` so an edit keeps them. */
export function queuedText(input: UserInputPart[]): string {
  return input
    .map((part) => (part.type === "text" ? (part.text ?? "") : `@${part.name}`))
    .join("")
    .trim();
}

let draft = $state<string | null>(null);
let textarea = $state<HTMLTextAreaElement | null>(null);

async function startEdit() {
  draft = queuedText(entry.input);
  await tick();
  textarea?.focus();
  textarea?.setSelectionRange(draft.length, draft.length);
}

function saveEdit() {
  const text = draft?.trim() ?? "";
  draft = null;
  if (!text || text === queuedText(entry.input)) return;
  onEdit([{ type: "text", text }]);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    draft = null;
  } else if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    saveEdit();
  }
}
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
  {#if draft !== null}
    <textarea
      bind:this={textarea}
      bind:value={draft}
      onkeydown={onKeydown}
      onblur={saveEdit}
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
        aria-label="Edit queued message"
        onclick={startEdit}
        class="btn-icon btn-icon-sm rounded-md text-surface-600-400 transition hover:preset-tonal"
      >
        <Pencil size={14} />
      </TooltipButton>
      <TooltipButton
        label="Remove queued message"
        aria-label="Remove queued message"
        onclick={onCancel}
        class="btn-icon btn-icon-sm rounded-md text-surface-600-400 transition hover:text-error-500 hover:preset-tonal"
      >
        <X size={14} />
      </TooltipButton>
    </div>
  {/if}
</div>
