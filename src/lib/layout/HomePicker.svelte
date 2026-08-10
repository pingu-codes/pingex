<script lang="ts">
import { AlertTriangle, Check, CheckCircle2, FolderOpen, House, Loader, Plus, Terminal, X } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import type { LaunchState } from "$lib/types";

let {
  launchState,
  busy = false,
  error = null,
  onSelect,
  onBrowse,
  onRemove,
  onSetBinary,
}: {
  launchState: LaunchState;
  busy?: boolean;
  error?: string | null;
  onSelect: (path: string) => void;
  onBrowse: () => Promise<string | null>;
  onRemove: (path: string) => void;
  /** Save a Codex CLI path; rejects with a message when it cannot be run. */
  onSetBinary: (path: string) => Promise<void>;
} = $props();

// Opening a home creates it on disk and spawns the CLI, so a missing binary
// blocks the picker outright and the form below is the only way forward.
const binary = $derived(launchState.codexBinaryStatus);
let binaryPath = $state("");
let editingBinary = $state(false);
let savingBinary = $state(false);
let binaryError = $state<string | null>(null);

const binaryFormOpen = $derived(editingBinary || !binary.found);
/** No home can be opened while the CLI is unusable. */
const locked = $derived(busy || !binary.found);

async function saveBinary() {
  const trimmed = binaryPath.trim();
  if (!trimmed || savingBinary) return;
  savingBinary = true;
  binaryError = null;
  try {
    await onSetBinary(trimmed);
    editingBinary = false;
    binaryPath = "";
  } catch (cause) {
    binaryError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    savingBinary = false;
  }
}

// A path picked via the dialog or typed manually, awaiting explicit
// confirmation before it becomes the active home.
let pendingPath = $state<string | null>(null);
let rawPath = $state("");
let browsing = $state(false);

async function browse() {
  browsing = true;
  try {
    const path = await onBrowse();
    if (path) {
      pendingPath = path;
      rawPath = "";
    }
  } finally {
    browsing = false;
  }
}

function useRawPath() {
  const trimmed = rawPath.trim();
  if (!trimmed) return;
  pendingPath = trimmed;
}

interface HomeOption {
  path: string;
  lastUsed: number | null;
  exists: boolean;
  isDefault: boolean;
  removable: boolean;
}

// Recent homes (newest first) plus the built-in `~/.codex` default, deduped by
// path so a default that is also a recent only appears once.
const options = $derived.by(() => {
  const seen = new Set<string>();
  const list: HomeOption[] = [];
  for (const home of launchState.recentHomes) {
    if (seen.has(home.path)) continue;
    seen.add(home.path);
    list.push({
      path: home.path,
      lastUsed: home.lastUsed,
      exists: home.exists,
      isDefault: home.path === launchState.defaultHome,
      removable: true,
    });
  }
  if (!seen.has(launchState.defaultHome)) {
    list.push({
      path: launchState.defaultHome,
      lastUsed: null,
      exists: true,
      isDefault: true,
      removable: false,
    });
  }
  return list;
});

const relativeTime = (timestamp: number | null) => {
  if (timestamp === null) return null;
  const days = Math.floor((Date.now() / 1000 - timestamp) / 86400);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  return `${days}d ago`;
};
</script>

<div class="grid h-full place-items-center overflow-y-auto bg-surface-50-950 text-surface-950-50" data-testid="home-picker">
  <div class="w-full max-w-md px-6 py-10">
    <div class="flex items-center gap-3">
      <div class="grid size-10 place-items-center rounded-xl preset-tonal-primary">
        <House size={20} strokeWidth={1.8} />
      </div>
      <div>
        <h1 class="text-lg font-semibold tracking-[-0.02em]">Choose a Codex home</h1>
        <p class="mt-0.5 text-xs text-surface-500">Pick where to boot. Your choice is remembered for next time.</p>
      </div>
    </div>

    {#if !binary.found}
      <div
        data-testid="binary-missing"
        class="mt-5 flex gap-3 rounded-xl border border-warning-500/40 bg-warning-500/5 px-3 py-2.5"
      >
        <AlertTriangle size={17} strokeWidth={1.8} class="mt-0.5 shrink-0 text-warning-500" />
        <div class="min-w-0 text-xs leading-5">
          <div class="font-medium">Codex CLI not found</div>
          <p class="text-surface-600-400">{binary.message ?? `Could not run ${binary.binary}.`}</p>
        </div>
      </div>
    {/if}

    <div class="mt-6 space-y-1.5" aria-busy={busy}>
      {#each options as option (option.path)}
        <div class="group/row relative">
          <button
            onclick={() => onSelect(option.path)}
            disabled={locked}
            data-testid="home-option"
            class="flex w-full items-center gap-3 rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2.5 text-left transition hover:preset-tonal disabled:pointer-events-none disabled:opacity-60 {option.exists ? '' : 'opacity-60'} {option.removable ? 'pr-9' : ''}"
          >
            <House size={17} strokeWidth={1.7} class="shrink-0 text-surface-500" />
            <div class="min-w-0 flex-1">
              <div class="flex items-center gap-1.5">
                <span class="truncate font-mono text-sm">{option.path}</span>
                {#if option.isDefault}
                  <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium">Default</span>
                {/if}
              </div>
              <div class="text-[11px] text-surface-500">
                {#if !option.exists}
                  Not found on disk
                {:else if relativeTime(option.lastUsed)}
                  Last used {relativeTime(option.lastUsed)}
                {:else}
                  ~/.codex
                {/if}
              </div>
            </div>
            {#if option.path === launchState.codexHome}
              <Check size={15} class="shrink-0 text-primary-500" />
            {/if}
          </button>
          {#if option.removable}
            <TooltipButton
              label="Remove from recents"
              onclick={() => onRemove(option.path)}
              disabled={busy}
              aria-label="Remove {option.path} from recents"
              data-testid="remove-home"
              class="absolute top-1/2 right-2 -translate-y-1/2 rounded-md p-1 text-surface-500 opacity-0 transition group-hover/row:opacity-100 focus-visible:opacity-100 hover:preset-tonal disabled:pointer-events-none"
            >
              <X size={14} />
            </TooltipButton>
          {/if}
        </div>
      {/each}
    </div>

    {#if pendingPath}
      <div
        data-testid="pending-home"
        class="mt-3 flex items-center gap-3 rounded-xl border border-success-500/40 bg-success-500/5 px-3 py-2.5"
      >
        <CheckCircle2 size={17} strokeWidth={1.8} class="shrink-0 text-success-500" />
        <span class="min-w-0 flex-1 truncate font-mono text-sm">{pendingPath}</span>
        <button
          onclick={() => onSelect(pendingPath!)}
          disabled={locked}
          data-testid="confirm-add-home"
          class="btn btn-sm shrink-0 preset-filled-primary-500 disabled:pointer-events-none disabled:opacity-60"
        >
          Add home
        </button>
        <TooltipButton
          label="Clear selection"
          onclick={() => (pendingPath = null)}
          disabled={busy}
          aria-label="Clear selection"
          class="shrink-0 rounded-md p-1 text-surface-500 transition hover:preset-tonal disabled:pointer-events-none disabled:opacity-60"
        >
          <X size={14} />
        </TooltipButton>
      </div>
    {:else}
      <button
        onclick={browse}
        disabled={locked || browsing}
        class="mt-3 flex w-full items-center justify-center gap-2 rounded-xl border border-dashed border-surface-300-700 px-4 py-3 text-sm text-surface-600-400 transition hover:border-surface-400-600 hover:text-surface-800-200 disabled:pointer-events-none disabled:opacity-60"
      >
        {#if busy || browsing}
          <Loader size={15} class="animate-spin" />
          Opening…
        {:else}
          <Plus size={15} />
          Browse for a folder
          <FolderOpen size={15} class="opacity-0" />
        {/if}
      </button>

      <!-- The native dialog hides dotfolders, so allow typing a path directly. -->
      <form
        class="mt-2 flex items-center gap-2"
        onsubmit={(event) => {
          event.preventDefault();
          useRawPath();
        }}
      >
        <input
          bind:value={rawPath}
          disabled={locked}
          placeholder="Or type a path, e.g. ~/.codex-work"
          data-testid="raw-home-path"
          class="input min-w-0 flex-1 rounded-xl px-3 py-2 font-mono text-sm"
          spellcheck="false"
          autocomplete="off"
        />
        <button
          type="submit"
          disabled={locked || !rawPath.trim()}
          data-testid="use-raw-path"
          class="btn btn-sm shrink-0 preset-tonal disabled:pointer-events-none disabled:opacity-60"
        >
          Use path
        </button>
      </form>
    {/if}

    {#if error}
      <div class="card preset-tonal-error mt-4 px-3 py-2 text-xs">{error}</div>
    {/if}

    <!-- The CLI the app spawns. A bundled app launched from Finder inherits a
         bare PATH, so a Homebrew/npm install often needs spelling out here. -->
    <div class="mt-5 border-t border-surface-200-800 pt-3" data-testid="binary-row">
      <div class="flex items-center gap-2 text-[11px] text-surface-500">
        <Terminal size={13} strokeWidth={1.7} class="shrink-0" />
        <span class="min-w-0 flex-1 truncate font-mono">{binary.resolved ?? binary.binary}</span>
        {#if !binaryFormOpen}
          <button
            onclick={() => {
              editingBinary = true;
              binaryPath = binary.resolved ?? binary.binary;
            }}
            data-testid="edit-binary"
            class="shrink-0 rounded-md px-1.5 py-0.5 transition hover:preset-tonal"
          >
            Change
          </button>
        {/if}
      </div>

      {#if binaryFormOpen}
        <form
          class="mt-2 flex items-center gap-2"
          onsubmit={(event) => {
            event.preventDefault();
            saveBinary();
          }}
        >
          <input
            bind:value={binaryPath}
            disabled={savingBinary}
            placeholder="Path to codex, e.g. /opt/homebrew/bin/codex"
            data-testid="binary-path"
            class="input min-w-0 flex-1 rounded-xl px-3 py-2 font-mono text-sm"
            spellcheck="false"
            autocomplete="off"
          />
          <button
            type="submit"
            disabled={savingBinary || !binaryPath.trim()}
            data-testid="save-binary"
            class="btn btn-sm shrink-0 preset-filled-primary-500 disabled:pointer-events-none disabled:opacity-60"
          >
            {savingBinary ? "Checking…" : "Use binary"}
          </button>
          {#if binary.found}
            <TooltipButton
              label="Cancel"
              type="button"
              onclick={() => {
                editingBinary = false;
                binaryError = null;
              }}
              aria-label="Cancel binary change"
              class="shrink-0 rounded-md p-1 text-surface-500 transition hover:preset-tonal"
            >
              <X size={14} />
            </TooltipButton>
          {/if}
        </form>
        {#if binaryError}
          <div class="card preset-tonal-error mt-2 px-3 py-2 text-xs" data-testid="binary-error">{binaryError}</div>
        {/if}
      {/if}
    </div>
  </div>
</div>
