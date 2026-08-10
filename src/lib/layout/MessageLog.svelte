<script lang="ts">
import { ArrowDownLeft, ArrowUpRight, Copy, RefreshCw, Trash2 } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  type DirectionFilter,
  describeMessage,
  filterMessages,
  formatPayload,
  formatTime,
  messagesToText,
} from "$lib/layout/messageLog";
import { messageLog } from "$lib/layout/messageLogPrefs.svelte";
import { copyText } from "$lib/services/api";

let query = $state("");
let direction = $state<DirectionFilter>("all");
let expanded = $state<Set<number>>(new Set());

const visible = $derived(filterMessages(messageLog.messages, { query, direction }));

function toggle(seq: number) {
  const next = new Set(expanded);
  if (!next.delete(seq)) next.add(seq);
  expanded = next;
}

// Newest first: a live log is read from the top.
const ordered = $derived([...visible].reverse());

/** Tailwind tint per message kind, so the shape of an exchange is scannable. */
const kindClass: Record<string, string> = {
  request: "preset-tonal-primary",
  response: "preset-tonal-success",
  serverRequest: "preset-tonal-warning",
  error: "preset-tonal-error",
  notification: "preset-tonal",
};
</script>

<div class="mt-4" data-testid="message-log">
  <div class="flex flex-wrap items-center gap-2">
    <input
      bind:value={query}
      placeholder="Filter messages"
      aria-label="Filter messages"
      class="input min-w-40 flex-1 text-sm"
    />
    <select bind:value={direction} aria-label="Direction" class="select w-36 text-sm">
      <option value="all">Both ways</option>
      <option value="out">To agent</option>
      <option value="in">From agent</option>
    </select>
    <TooltipButton
      label="Reload from the buffer"
      type="button"
      aria-label="Refresh message log"
      onclick={() => messageLog.refresh()}
      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    >
      <RefreshCw size={14} />
    </TooltipButton>
    <TooltipButton
      label="Copy the filtered messages"
      type="button"
      aria-label="Copy message log"
      onclick={() => copyText(messagesToText(visible))}
      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    >
      <Copy size={14} />
    </TooltipButton>
    <TooltipButton
      label="Clear the buffer"
      type="button"
      aria-label="Clear message log"
      onclick={() => messageLog.clear()}
      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    >
      <Trash2 size={14} />
    </TooltipButton>
  </div>

  {#if messageLog.error}
    <div class="card preset-tonal-error mt-3 px-3 py-2 text-xs">{messageLog.error}</div>
  {/if}

  <div class="mt-3 max-h-[420px] overflow-y-auto rounded-lg border border-surface-200-800">
    {#each ordered as message (message.seq)}
      {@const open = expanded.has(message.seq)}
      <div class="border-b border-surface-200-800 last:border-b-0" data-testid="message-log-row">
        <button
          type="button"
          onclick={() => toggle(message.seq)}
          aria-expanded={open}
          class="flex w-full items-center gap-2 px-3 py-2 text-left hover:preset-tonal"
        >
          {#if message.direction === "out"}
            <ArrowUpRight size={13} class="shrink-0 text-primary-500" />
          {:else}
            <ArrowDownLeft size={13} class="shrink-0 text-surface-500" />
          {/if}
          <span class="font-mono text-[11px] text-surface-500">{formatTime(message.at)}</span>
          <span class="chip {kindClass[message.kind] ?? 'preset-tonal'} shrink-0 text-[10px]">{message.kind}</span>
          <span class="truncate font-mono text-xs">{describeMessage(message)}</span>
          {#if message.id !== null && message.method}
            <span class="shrink-0 font-mono text-[10px] text-surface-500">#{message.id}</span>
          {/if}
        </button>
        {#if open}
          <pre
            class="max-h-64 overflow-auto bg-surface-100-900 px-3 py-2 font-mono text-[11px] leading-4 whitespace-pre-wrap">{formatPayload(
              message.payload,
            )}</pre>
          {#if message.truncated}
            <p class="px-3 pb-2 text-[11px] text-surface-500">Payload truncated — too large to keep in full.</p>
          {/if}
        {/if}
      </div>
    {:else}
      <p class="px-3 py-6 text-center text-xs text-surface-500">
        {messageLog.messages.length === 0
          ? "No messages captured yet. Send something to the agent and it will show up here."
          : `No messages match "${query}".`}
      </p>
    {/each}
  </div>
</div>
