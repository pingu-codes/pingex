<script lang="ts">
import { Check, Copy } from "@lucide/svelte";
import type { Snippet } from "svelte";
import type { ThreadSummary } from "$lib/types";

let {
  thread,
  codexHome,
  children,
}: {
  thread: ThreadSummary | null;
  codexHome: string | null;
  children: Snippet;
} = $props();

let open = $state(false);
let copiedKey = $state<string | null>(null);
let openTimer: ReturnType<typeof setTimeout> | null = null;
let closeTimer: ReturnType<typeof setTimeout> | null = null;

const rows = $derived(
  thread
    ? (
        [
          ["Thread ID", thread.id],
          ["Parent ID", thread.parentThreadId ?? null],
          [
            "Agent",
            thread.agentNickname
              ? `${thread.agentNickname}${thread.agentRole ? ` (${thread.agentRole})` : ""}`
              : (thread.agentRole ?? null),
          ],
          ["Status", thread.status],
          ["CWD", thread.cwd],
          ["Codex home", codexHome],
          ["Updated", new Date(thread.updatedAt * 1000).toLocaleString()],
        ] satisfies [string, string | null][]
      ).filter((row): row is [string, string] => row[1] !== null && row[1] !== "")
    : [],
);

function show() {
  if (!thread) return;
  if (closeTimer) clearTimeout(closeTimer);
  closeTimer = null;
  if (open || openTimer) return;
  openTimer = setTimeout(() => {
    openTimer = null;
    open = true;
  }, 450);
}

function hide() {
  if (openTimer) clearTimeout(openTimer);
  openTimer = null;
  if (!open || closeTimer) return;
  closeTimer = setTimeout(() => {
    closeTimer = null;
    open = false;
    copiedKey = null;
  }, 200);
}

async function copy(key: string, value: string) {
  try {
    await navigator.clipboard.writeText(value);
    copiedKey = key;
    setTimeout(() => {
      if (copiedKey === key) copiedKey = null;
    }, 1200);
  } catch {
    // Clipboard unavailable; nothing to surface in a debug hover.
  }
}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="relative min-w-0" class:pointer-events-none={!thread} onmouseenter={show} onmouseleave={hide}>
  {@render children()}
  {#if open && thread}
    <div
      class="card absolute left-0 top-full z-[80] mt-1.5 w-[26rem] max-w-[80vw] select-text border border-surface-200-800 bg-surface-50-950 p-2 shadow-xl"
      role="tooltip"
      aria-label="Thread debug info"
    >
      <div class="mb-1 px-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Thread debug</div>
      {#each rows as [label, value] (label)}
        <button
          type="button"
          onclick={() => copy(label, value)}
          title="Click to copy"
          class="group flex w-full items-baseline gap-2 rounded px-1 py-0.5 text-left hover:preset-tonal"
        >
          <span class="w-20 shrink-0 text-[10px] text-surface-500">{label}</span>
          <span class="min-w-0 flex-1 break-all font-mono text-[11px] text-surface-700-300">{value}</span>
          {#if copiedKey === label}
            <Check size={11} class="shrink-0 self-center text-primary-500" />
          {:else}
            <Copy size={11} class="shrink-0 self-center text-surface-400-600 opacity-0 transition group-hover:opacity-100" />
          {/if}
        </button>
      {/each}
    </div>
  {/if}
</div>
