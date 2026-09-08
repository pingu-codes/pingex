<script lang="ts">
import { ArrowDownToLine, ArrowUpFromLine, CloudDownload } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import type { SyncOperation } from "./sync";

let {
  upstream = null,
  ahead = 0,
  behind = 0,
  detached = false,
  busy = null,
  onSync,
}: {
  upstream?: string | null;
  ahead?: number;
  behind?: number;
  detached?: boolean;
  /** The operation currently running, if any. */
  busy?: SyncOperation | null;
  onSync: (operation: SyncOperation) => void;
} = $props();

const disabled = $derived(busy !== null);
</script>

<div class="flex items-center gap-1" role="group" aria-label="Sync">
  <TooltipButton label="Fetch from remote" onclick={() => onSync("fetch")} {disabled} aria-label="Fetch" class="btn btn-sm preset-tonal disabled:opacity-40">
    <CloudDownload size={13} class={busy === "fetch" ? "animate-pulse" : ""} />
    Fetch
  </TooltipButton>
  {#if upstream}
    <TooltipButton label="Pull (fast-forward only)" onclick={() => onSync("pull")} {disabled} aria-label="Pull" class="btn btn-sm preset-tonal disabled:opacity-40">
      <ArrowDownToLine size={13} class={busy === "pull" ? "animate-pulse" : ""} />
      Pull{#if behind > 0}<span class="font-mono text-[10px]">↓{behind}</span>{/if}
    </TooltipButton>
    <TooltipButton label="Push to {upstream}" onclick={() => onSync("push")} {disabled} aria-label="Push" class="btn btn-sm {ahead > 0 ? 'preset-filled-primary-500' : 'preset-tonal'} disabled:opacity-40">
      <ArrowUpFromLine size={13} class={busy === "push" ? "animate-pulse" : ""} />
      Push{#if ahead > 0}<span class="font-mono text-[10px]">↑{ahead}</span>{/if}
    </TooltipButton>
  {:else if !detached}
    <TooltipButton label="Push and set origin as upstream" onclick={() => onSync("publish")} {disabled} aria-label="Publish branch" class="btn btn-sm preset-filled-primary-500 disabled:opacity-40">
      <ArrowUpFromLine size={13} class={busy === "publish" ? "animate-pulse" : ""} />
      Publish branch
    </TooltipButton>
  {/if}
</div>
