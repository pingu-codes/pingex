<script lang="ts">
import { Archive, ArchiveRestore, ChevronDown } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { listThreadsPage, unarchiveThread } from "$lib/services/api";
import type { ArchivedThread, BootstrapData, ThreadSummary } from "$lib/types";
import { relativeTime } from "$lib/utils/time";

let {
  onSelectArchived,
  onUnarchived,
}: {
  onSelectArchived?: (thread: ArchivedThread) => void;
  onUnarchived?: (data: BootstrapData) => void;
} = $props();

const PAGE_SIZE = 50;

let archivedOpen = $state(false);
let archived = $state<ThreadSummary[] | null>(null);
let cursor = $state<string | null>(null);
let loadingMore = $state(false);
let archivedError = $state<string | null>(null);

// Archived history can be large; page it lazily with `Load more` rather than
// loading a fixed cap up front so the sidebar stays responsive.
async function loadPage() {
  loadingMore = true;
  try {
    const page = await listThreadsPage(cursor, PAGE_SIZE, true, null);
    archived = [...(archived ?? []), ...page.items];
    cursor = page.nextCursor;
    archivedError = null;
  } catch (cause) {
    archivedError = cause instanceof Error ? cause.message : String(cause);
    if (archived === null) archived = [];
  } finally {
    loadingMore = false;
  }
}

async function toggleArchived() {
  archivedOpen = !archivedOpen;
  if (archivedOpen && archived === null) await loadPage();
}

function asArchivedThread(thread: ThreadSummary): ArchivedThread {
  return { id: thread.id, title: thread.title, cwd: thread.cwd, updatedAt: thread.updatedAt };
}

async function unarchive(thread: ThreadSummary) {
  try {
    const data = await unarchiveThread(thread.id);
    archived = (archived ?? []).filter((entry) => entry.id !== thread.id);
    onUnarchived?.(data);
  } catch (cause) {
    archivedError = cause instanceof Error ? cause.message : String(cause);
  }
}
</script>

<div class="border-t border-surface-200-800 px-2 py-1.5">
  <button
    onclick={toggleArchived}
    class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500 hover:preset-tonal"
  >
    <Archive size={12} />
    <span class="flex-1">Archived</span>
    {#if archived && archived.length > 0}
      <span class="text-[10px] font-normal normal-case tracking-normal text-surface-500">{archived.length}</span>
    {/if}
    <ChevronDown size={12} class="transition {archivedOpen ? '' : '-rotate-90'}" />
  </button>
  {#if archivedOpen}
    <div class="max-h-48 overflow-y-auto pb-1">
      {#if archivedError}
        <p class="px-2 py-1.5 text-xs text-error-500">{archivedError}</p>
      {:else if archived === null}
        <p class="px-2 py-1.5 text-xs text-surface-500">Loading…</p>
      {:else if archived.length === 0}
        <p class="px-2 py-1.5 text-xs text-surface-500">No archived threads</p>
      {:else}
        {#each archived as thread (thread.id)}
          <div class="group/archived relative">
            <button
              onclick={() => onSelectArchived?.(asArchivedThread(thread))}
              class="flex w-full items-center gap-2 rounded-md py-1.5 pl-2 pr-7 text-left text-[12px] text-surface-600-400 hover:preset-tonal"
            >
              <span class="min-w-0 flex-1 truncate" title={thread.title}>{thread.title}</span>
              <span class="shrink-0 text-[10px] text-surface-500 group-hover/archived:opacity-0">{relativeTime(thread.updatedAt)}</span>
            </button>
            <TooltipButton
              label="Unarchive"
              aria-label={`Unarchive ${thread.title}`}
              onclick={() => unarchive(thread)}
              class="absolute right-0.5 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-300-700 hover:text-surface-900-100 group-hover/archived:opacity-100"
            >
              <ArchiveRestore size={12} />
            </TooltipButton>
          </div>
        {/each}
        {#if cursor}
          <button
            onclick={loadPage}
            disabled={loadingMore}
            class="mt-0.5 w-full rounded-md px-2 py-1 text-[11px] font-medium text-primary-500 hover:preset-tonal disabled:opacity-50"
          >
            {loadingMore ? "Loading…" : "Load more"}
          </button>
        {/if}
      {/if}
    </div>
  {/if}
</div>
