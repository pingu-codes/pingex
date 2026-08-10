<script lang="ts">
import { Archive, MessageSquare, Search, X } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  countLabel,
  emptyGroup,
  filterChipLabel,
  hasMore,
  noMatchLabel,
  type SearchGroup,
  searchState,
} from "$lib/layout/sidebarSearch";
import { searchThreads } from "$lib/services/api";
import type { Project, ThreadSearchFilter, ThreadSearchItem } from "$lib/types";
import { relativeTime } from "$lib/utils/time";

let {
  projects,
  currentProjectPath = null,
  currentProjectName = null,
  onOpenResult,
  onClose,
}: {
  projects: Project[];
  currentProjectPath?: string | null;
  currentProjectName?: string | null;
  onOpenResult: (item: ThreadSearchItem) => void;
  onClose: () => void;
} = $props();

let query = $state("");
let scopeOverride = $state<boolean | null>(null);
let active = $state<SearchGroup>(emptyGroup());
let archived = $state<SearchGroup>(emptyGroup());
let error = $state<string | null>(null);
let loading = $state(false);
let input = $state<HTMLInputElement | null>(null);

// A monotonically increasing generation guards against out-of-order responses:
// only results carrying the latest generation are applied.
let generation = 0;
let debounce: ReturnType<typeof setTimeout> | null = null;

// Default to searching within the current project when there is one; the chip
// toggles this to "all projects".
const scoped = $derived(scopeOverride ?? Boolean(currentProjectPath));
const filterProject = $derived(scoped ? currentProjectPath : null);
const chipLabel = $derived(scoped ? filterChipLabel(currentProjectName) : null);
const uiState = $derived(searchState({ query, active, archived, error, loading }));

function projectNameFor(cwd: string): string {
  const match = projects.find((project) => cwd.startsWith(project.path));
  return match?.name ?? cwd.split("/").filter(Boolean).pop() ?? cwd;
}

function filterFor(isArchived: boolean): ThreadSearchFilter {
  return { archived: isArchived, projectPath: filterProject };
}

async function runInitial() {
  const mine = ++generation;
  error = null;
  loading = true;
  active = { ...emptyGroup(), loading: true };
  archived = { ...emptyGroup(), loading: true };
  try {
    const [activePage, archivedPage] = await Promise.all([
      searchThreads(query, null, filterFor(false), mine),
      searchThreads(query, null, filterFor(true), mine),
    ]);
    if (mine !== generation) return;
    active = {
      items: activePage.items,
      total: activePage.total,
      cursor: activePage.nextCursor,
      loading: false,
    };
    archived = {
      items: archivedPage.items,
      total: archivedPage.total,
      cursor: archivedPage.nextCursor,
      loading: false,
    };
  } catch (cause) {
    if (mine !== generation) return;
    error = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (mine === generation) loading = false;
  }
}

async function loadMore(isArchived: boolean) {
  const group = isArchived ? archived : active;
  if (!hasMore(group) || group.loading) return;
  const mine = generation;
  if (isArchived) archived = { ...archived, loading: true };
  else active = { ...active, loading: true };
  try {
    const page = await searchThreads(query, group.cursor, filterFor(isArchived), mine);
    if (mine !== generation) return;
    const next: SearchGroup = {
      items: [...group.items, ...page.items],
      total: page.total,
      cursor: page.nextCursor,
      loading: false,
    };
    if (isArchived) archived = next;
    else active = next;
  } catch (cause) {
    if (mine !== generation) return;
    error = cause instanceof Error ? cause.message : String(cause);
    if (isArchived) archived = { ...archived, loading: false };
    else active = { ...active, loading: false };
  }
}

function scheduleSearch() {
  if (debounce) clearTimeout(debounce);
  debounce = setTimeout(runInitial, 150);
}

// Re-run whenever the query text or project scope changes.
$effect(() => {
  // Touch the reactive dependencies so the effect tracks them.
  void query;
  void filterProject;
  scheduleSearch();
});

$effect(() => {
  input?.focus();
});

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    onClose();
  }
}

function toggleScope() {
  scopeOverride = !scoped;
}

function selectResult(item: ThreadSearchItem) {
  onOpenResult(item);
  onClose();
}
</script>

<div class="flex min-h-0 flex-1 flex-col" role="search">
  <div class="px-2 pt-2">
    <div class="flex items-center gap-2 rounded-md bg-surface-200-800 px-2 py-1.5">
      <Search size={14} class="shrink-0 text-surface-500" />
      <!-- svelte-ignore a11y_autofocus -->
      <input
        bind:this={input}
        bind:value={query}
        onkeydown={onKeydown}
        type="text"
        placeholder="Search threads…"
        aria-label="Search threads"
        class="min-w-0 flex-1 bg-transparent text-[13px] outline-none placeholder:text-surface-500"
      />
      <TooltipButton
        label="Close search"
        onclick={onClose}
        aria-label="Close search"
        class="btn-icon btn-icon-sm shrink-0 text-surface-500 hover:preset-tonal"
      >
        <X size={14} />
      </TooltipButton>
    </div>
    {#if currentProjectName}
      <div class="mt-1.5 flex items-center gap-1.5 px-0.5">
        <button
          onclick={toggleScope}
          class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-[10px] font-medium transition {scoped
            ? 'preset-filled-primary-500'
            : 'bg-surface-200-800 text-surface-600-400 hover:preset-tonal'}"
          title={scoped ? `Searching in ${chipLabel}` : "Search all projects"}
        >
          {#if scoped}
            {chipLabel}
            <X size={10} />
          {:else}
            All projects
          {/if}
        </button>
      </div>
    {/if}
  </div>

  <div class="mt-2 min-h-0 flex-1 overflow-y-auto px-2 pb-3">
    {#if uiState === "error"}
      <p class="px-2 py-3 text-xs text-error-500">Search is unavailable: {error}</p>
    {:else if uiState === "idle"}
      <p class="px-2 py-3 text-xs text-surface-500">Type to search all threads and archived history.</p>
    {:else if uiState === "loading"}
      <div class="space-y-2 px-1 py-2" aria-label="Searching">
        <div class="placeholder h-7 animate-pulse rounded-md"></div>
        <div class="placeholder h-7 animate-pulse rounded-md opacity-70"></div>
      </div>
    {:else if uiState === "empty"}
      <p class="px-2 py-3 text-xs text-surface-500">{noMatchLabel(query)}</p>
    {:else}
      {#if active.total > 0}
        <div class="mb-2">
          <div class="flex items-center justify-between px-2 py-1">
            <span class="inline-flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">
              <MessageSquare size={11} /> Active
            </span>
            <span class="text-[10px] text-surface-500">{countLabel(active.items.length, active.total)}</span>
          </div>
          {#each active.items as item (item.id)}
            <button
              onclick={() => selectResult(item)}
              class="flex w-full flex-col gap-0.5 rounded-md px-2 py-1.5 text-left hover:preset-tonal"
            >
              <span class="flex items-center gap-2">
                <span class="min-w-0 flex-1 truncate text-[12px] text-surface-800-200" title={item.title}>{item.title}</span>
                <span class="shrink-0 text-[10px] text-surface-500">{relativeTime(item.updatedAt)}</span>
              </span>
              <span class="truncate text-[10px] text-surface-500">{projectNameFor(item.cwd)}</span>
            </button>
          {/each}
          {#if hasMore(active)}
            <button
              onclick={() => loadMore(false)}
              disabled={active.loading}
              class="mt-1 w-full rounded-md px-2 py-1 text-[11px] font-medium text-primary-500 hover:preset-tonal disabled:opacity-50"
            >
              {active.loading ? "Loading…" : "Load more"}
            </button>
          {/if}
        </div>
      {/if}

      {#if archived.total > 0}
        <div>
          <div class="flex items-center justify-between px-2 py-1">
            <span class="inline-flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">
              <Archive size={11} /> Archived
            </span>
            <span class="text-[10px] text-surface-500">{countLabel(archived.items.length, archived.total)}</span>
          </div>
          {#each archived.items as item (item.id)}
            <button
              onclick={() => selectResult(item)}
              class="flex w-full flex-col gap-0.5 rounded-md px-2 py-1.5 text-left hover:preset-tonal"
            >
              <span class="flex items-center gap-2">
                <span class="min-w-0 flex-1 truncate text-[12px] text-surface-600-400" title={item.title}>{item.title}</span>
                <span class="shrink-0 text-[10px] text-surface-500">{relativeTime(item.updatedAt)}</span>
              </span>
              <span class="truncate text-[10px] text-surface-500">{projectNameFor(item.cwd)}</span>
            </button>
          {/each}
          {#if hasMore(archived)}
            <button
              onclick={() => loadMore(true)}
              disabled={archived.loading}
              class="mt-1 w-full rounded-md px-2 py-1 text-[11px] font-medium text-primary-500 hover:preset-tonal disabled:opacity-50"
            >
              {archived.loading ? "Loading…" : "Load more"}
            </button>
          {/if}
        </div>
      {/if}
    {/if}
  </div>
</div>
