<script lang="ts">
import { OctagonX } from "@lucide/svelte";
import { elapsedLabel } from "$lib/services/agentRuns.svelte";
import { processClock, type RunningProcess } from "$lib/services/processes.svelte";

let {
  process,
  onStopTurn,
}: {
  process: RunningProcess | null;
  /** Present only when the process belongs to the open thread. Interrupts the whole turn. */
  onStopTurn?: (process: RunningProcess) => void;
} = $props();

let output = $state<HTMLElement | null>(null);

const statusLabel = $derived(
  process?.status === "running"
    ? "Running"
    : process?.status === "completed"
      ? "Finished"
      : process?.status === "failed"
        ? "Failed"
        : "Interrupted",
);
const statusClass = $derived(
  process?.status === "running"
    ? "text-success-500"
    : process?.status === "failed"
      ? "text-error-500"
      : "text-surface-500",
);
const timing = $derived.by(() => {
  if (!process) return "";
  const end = process.finishedAt ?? (processClock.now || Date.now());
  return elapsedLabel(process.startedAt, end);
});

// Follow the stream while it is running, but only when already near the
// bottom — the same rule the transcript uses.
$effect(() => {
  if (process?.status !== "running" || !output) return;
  void process.output.length;
  const element = output;
  const nearBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 120;
  if (nearBottom) {
    requestAnimationFrame(() => {
      element.scrollTop = element.scrollHeight;
    });
  }
});
</script>

{#if !process}
  <p class="text-xs text-surface-500">This process is no longer tracked.</p>
{:else}
  <div class="flex h-full min-h-0 flex-col gap-2">
    <div class="shrink-0 space-y-1 rounded-lg bg-surface-50-950 p-2.5">
      <div class="break-all font-mono text-xs leading-5">$ {process.command || "(command)"}</div>
      {#if process.cwd}
        <div class="truncate text-[10px] text-surface-500">{process.cwd}</div>
      {/if}
      <div class="flex items-center gap-2 text-[10px]">
        <span class="font-medium {statusClass}">
          {#if process.status === "running"}<span class="mr-1 inline-block size-1.5 animate-pulse rounded-full bg-success-500"></span>{/if}
          {statusLabel}
        </span>
        <span class="text-surface-500">{timing}</span>
        {#if process.exitCode != null}
          <span class="text-surface-500">exit {process.exitCode}</span>
        {/if}
      </div>
    </div>
    <pre
      bind:this={output}
      class="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-lg bg-surface-50-950 p-2.5 font-mono text-[11px] leading-5"
    >{process.output.trim() || "No output yet."}</pre>
    <p class="shrink-0 text-[10px] text-surface-500">Includes text Codex typed into the command.</p>
    {#if process.status === "running" && onStopTurn}
      <button
        class="btn btn-sm preset-tonal w-full shrink-0 text-error-500"
        onclick={() => onStopTurn(process)}
      >
        <OctagonX size={14} />
        Stop turn (stops all its work)
      </button>
    {/if}
  </div>
{/if}
