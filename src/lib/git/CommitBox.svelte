<script lang="ts">
import { AlertTriangle, Check, ChevronDown } from "@lucide/svelte";
import type { CommitResult } from "$lib/types";
import { type GitError, hintFor } from "./gitErrors";

let {
  stagedCount,
  identityName = null,
  identityEmail = null,
  busy = false,
  lastCommit = null,
  error = null,
  onCommit,
}: {
  stagedCount: number;
  identityName?: string | null;
  identityEmail?: string | null;
  busy?: boolean;
  lastCommit?: CommitResult | null;
  error?: GitError | null;
  /** Resolves once the commit is made; the caller refreshes status. */
  onCommit: (message: string) => Promise<boolean>;
} = $props();

let message = $state("");
let showHookOutput = $state(false);

const hasIdentity = $derived(!!identityName && !!identityEmail);
const canCommit = $derived(!busy && stagedCount > 0 && message.trim().length > 0);

async function commit() {
  if (!canCommit) return;
  if (await onCommit(message)) message = "";
}

function onKeydown(event: KeyboardEvent) {
  if ((event.metaKey || event.ctrlKey) && event.key === "Enter") {
    event.preventDefault();
    void commit();
  }
}
</script>

<div class="rounded-xl border border-surface-200-800 bg-surface-100-900 p-3">
  <textarea
    bind:value={message}
    onkeydown={onKeydown}
    rows="3"
    placeholder={stagedCount > 0 ? "Commit message" : "Stage changes to commit"}
    aria-label="Commit message"
    disabled={busy}
    class="w-full resize-y rounded-md border border-surface-300-700 bg-surface-50-950 px-2.5 py-1.5 text-sm outline-none placeholder:text-surface-500 focus:border-primary-500"
  ></textarea>
  <div class="mt-2 flex items-center gap-2">
    <span class="min-w-0 flex-1 truncate text-[11px] text-surface-500">
      {#if hasIdentity}
        Committing as <span class="text-surface-700-300">{identityName} &lt;{identityEmail}&gt;</span>
      {:else}
        <span class="inline-flex items-center gap-1 text-warning-600 dark:text-warning-400"><AlertTriangle size={11} /> No git identity configured — set user.name and user.email.</span>
      {/if}
    </span>
    <button type="button" onclick={commit} disabled={!canCommit} class="btn btn-sm preset-filled-primary-500 disabled:opacity-40">
      {busy ? "Committing…" : `Commit${stagedCount > 0 ? ` ${stagedCount} file${stagedCount === 1 ? "" : "s"}` : ""}`}
    </button>
  </div>

  {#if error}
    <div class="mt-2 rounded-md preset-tonal-error px-3 py-2 text-xs">
      <div>{error.message}</div>
      {#if hintFor(error.kind)}<div class="mt-0.5 opacity-80">{hintFor(error.kind)}</div>{/if}
      {#if error.detail}<pre class="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[10px] leading-4 opacity-90">{error.detail}</pre>{/if}
    </div>
  {:else if lastCommit}
    <div class="mt-2 rounded-md bg-success-500/10 px-3 py-2 text-xs text-success-700 dark:text-success-300">
      <div class="flex items-center gap-1.5">
        <Check size={12} />
        <span class="font-mono">{lastCommit.shortHash}</span>
        <span class="min-w-0 flex-1 truncate">{lastCommit.subject}</span>
        {#if lastCommit.hookOutput}
          <button type="button" onclick={() => (showHookOutput = !showHookOutput)} class="inline-flex items-center gap-0.5 text-[10px] opacity-80 hover:opacity-100" aria-expanded={showHookOutput}>
            Hook output <ChevronDown size={11} class={showHookOutput ? "rotate-180" : ""} />
          </button>
        {/if}
      </div>
      {#if showHookOutput && lastCommit.hookOutput}
        <pre class="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[10px] leading-4 opacity-90">{lastCommit.hookOutput}</pre>
      {/if}
    </div>
  {/if}
</div>
