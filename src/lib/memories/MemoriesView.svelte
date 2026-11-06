<script lang="ts">
  import { Brain, ChevronDown, FolderOpen, Pencil, RefreshCw, Search, Trash2, TriangleAlert } from "@lucide/svelte";
  import { projects } from "$lib/app/appData.svelte";
  import RefreshIcon from "$lib/components/RefreshIcon.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import {
    defaultEntry,
    entryCount,
    filterMemories,
    findEntry,
    formatAge,
    isMarkdown,
    stripFrontmatter,
  } from "$lib/memories/memoriesHelpers";
  import { commands } from "$lib/services/homeRouting";
  import type { MemoryHome } from "$lib/types";
  import { copyCode } from "$lib/utils/copy";
  import { errorMessage } from "$lib/utils/errors";
  import { formatBytes } from "$lib/utils/format";
  import { renderMarkdown } from "$lib/utils/markdown";

  let homes = $state<MemoryHome[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let query = $state("");
  let selected = $state<{ homeId: string; path: string } | null>(null);
  /** File contents by path, as last read or saved. */
  let contents = $state<Record<string, string>>({});
  /** Unsaved edits by path; survives switching between memories. */
  let drafts = $state<Record<string, string>>({});
  let editing = $state(false);
  let saving = $state(false);
  let confirmDelete = $state(false);
  let collapsed = $state<Record<string, boolean>>({});

  const projectPaths = () => projects().map((project) => project.path);
  const filtered = $derived(filterMemories(homes, query));
  const current = $derived(findEntry(homes, selected?.homeId ?? null, selected?.path ?? null));
  const text = $derived(current ? contents[current.entry.path] : undefined);
  const draft = $derived(current ? drafts[current.entry.path] : undefined);
  const dirty = $derived(draft !== undefined && draft !== text);

  async function load() {
    loading = true;
    error = null;
    try {
      apply(await commands.listMemories(projectPaths()));
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  /** Take a fresh listing, keeping the selection while its file still exists. */
  function apply(next: MemoryHome[]) {
    homes = next;
    if (!findEntry(next, selected?.homeId ?? null, selected?.path ?? null)) select(defaultEntry(next));
  }

  function select(target: { homeId: string; path: string } | null) {
    selected = target;
    confirmDelete = false;
    editing = target ? drafts[target.path] !== undefined : false;
    if (target) void read(target);
  }

  async function read(target: { homeId: string; path: string }) {
    try {
      const body = await commands.readMemory(target.homeId, target.path);
      contents[target.path] = body;
    } catch (cause) {
      error = errorMessage(cause);
    }
  }

  function startEdit() {
    if (!current || text === undefined) return;
    drafts[current.entry.path] = drafts[current.entry.path] ?? text;
    editing = true;
  }

  function cancelEdit() {
    if (current) delete drafts[current.entry.path];
    editing = false;
  }

  async function save() {
    if (!current || draft === undefined) return;
    const { homeId } = current.home;
    const { path } = current.entry;
    // Captured: by the time the write returns the user may be on another memory.
    const text = draft;
    saving = true;
    error = null;
    try {
      const next = await commands.writeMemory(homeId, path, text, projectPaths());
      contents[path] = text;
      delete drafts[path];
      if (current?.entry.path === path) editing = false;
      apply(next);
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      saving = false;
    }
  }

  async function remove() {
    if (!current) return;
    const { homeId } = current.home;
    const { path } = current.entry;
    error = null;
    try {
      const next = await commands.deleteMemory(homeId, path, projectPaths());
      delete drafts[path];
      delete contents[path];
      // Only close the editor if it is still showing the deleted memory.
      if (current?.entry.path === path) {
        confirmDelete = false;
        editing = false;
      }
      apply(next);
    } catch (cause) {
      error = errorMessage(cause);
    }
  }

  async function toggle(home: MemoryHome, key: string, value: boolean) {
    error = null;
    try {
      apply(await commands.setMemoryToggle(home.homeId, key, value, projectPaths()));
    } catch (cause) {
      error = errorMessage(cause);
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (editing && (event.metaKey || event.ctrlKey) && event.key === "s") {
      event.preventDefault();
      void save();
    }
  }

  void load();
</script>

<svelte:window onkeydown={onKeydown} />

<div class="flex h-full min-h-0" data-testid="memories-view">
  <aside class="flex w-80 shrink-0 flex-col border-r border-surface-200-800">
    <div class="flex items-center gap-2 border-b border-surface-200-800 p-3">
      <div class="relative min-w-0 flex-1">
        <Search size={13} class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-surface-500" />
        <input
          class="input h-8 w-full pl-8 text-xs"
          type="search"
          placeholder="Search memories"
          aria-label="Search memories"
          bind:value={query}
        >
      </div>
      <TooltipButton
        label="Reload memories"
        onclick={load}
        aria-label="Reload memories"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      >
        <RefreshIcon spinning={loading} size={14} />
      </TooltipButton>
    </div>

    <div class="min-h-0 flex-1 overflow-y-auto p-2">
      {#each filtered as home (home.homeId)}
        <section class="mb-4">
          <div class="px-2 pb-1 pt-1">
            <div class="eyebrow">{home.label}</div>
            <div class="truncate text-[10px] text-surface-500" title={home.configDir}>{home.configDir}</div>
          </div>

          {#if home.error}
            <div class="mx-2 my-1 flex items-start gap-1.5 rounded-md preset-tonal-error px-2 py-1.5 text-[11px]">
              <TriangleAlert size={12} class="mt-0.5 shrink-0" />{home.error}
            </div>
          {/if}

          {#each home.toggles as setting (setting.key)}
            <div class="flex items-center gap-2 rounded-md px-2 py-1.5">
              <div class="min-w-0 flex-1">
                <div class="text-xs">{setting.label}</div>
                <div class="text-[10px] leading-4 text-surface-500">{setting.description}</div>
              </div>
              <Toggle
                checked={setting.value}
                label={`${home.label}: ${setting.label}`}
                onChange={(checked) => toggle(home, setting.key, checked)}
              />
            </div>
          {/each}

          {#if home.stores.length === 0 && !home.error}
            <div class="px-2 py-2 text-[11px] leading-4 text-surface-500">
              {query ? "No memories match." : "No memories yet. They appear here once the harness writes some."}
            </div>
          {/if}

          {#each home.stores as store (store.root)}
            {@const key = `${home.homeId}:${store.root}`}
            {@const open = !collapsed[key] || query.trim() !== ""}
            <div class="mt-1">
              <button
                type="button"
                class="flex w-full items-center gap-1.5 rounded-md px-2 py-1 text-left text-xs font-medium transition hover:preset-tonal"
                title={store.projectPath ?? store.root}
                onclick={() => (collapsed[key] = open)}
              >
                <ChevronDown size={12} class="shrink-0 text-surface-500 transition {open ? "" : "-rotate-90"}" />
                <span class="min-w-0 flex-1 truncate">{store.title}</span>
                <span class="text-[10px] tabular-nums text-surface-500"
                  >{store.entries.length}{store.entries.length !== store.total ? `/${store.total}` : ""}</span
                >
              </button>
              {#if open}
                <div class="ml-3 space-y-0.5 border-l border-surface-200-800 pl-1.5">
                  {#each store.entries as entry (entry.path)}
                    {@const active = selected?.homeId === home.homeId && selected.path === entry.path}
                    <button
                      type="button"
                      class="block w-full rounded-md px-2 py-1.5 text-left transition {active
                        ? "preset-tonal-primary"
                        : "hover:preset-tonal"}"
                      onclick={() => select({ homeId: home.homeId, path: entry.path })}
                    >
                      <div class="flex items-center gap-1.5">
                        <span class="min-w-0 flex-1 truncate text-xs">{entry.title}</span>
                        {#if drafts[entry.path] !== undefined}
                          <span class="size-1.5 shrink-0 rounded-full bg-warning-500" title="Unsaved changes"></span>
                        {/if}
                        {#if entry.kind}
                          <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 text-[10px] text-surface-600-400"
                            >{entry.kind}</span
                          >
                        {/if}
                      </div>
                      {#if entry.description}
                        <div class="mt-0.5 truncate text-[10px] text-surface-500">{entry.description}</div>
                      {/if}
                      {#if entry.unindexed}
                        <div class="mt-0.5 text-[10px] text-warning-600-400">Not in MEMORY.md</div>
                      {/if}
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </section>
      {/each}
    </div>
  </aside>

  <section class="flex min-w-0 flex-1 flex-col">
    {#if error}
      <div class="mx-5 mt-3 flex items-start gap-2 error-note">
        <TriangleAlert size={13} class="mt-0.5 shrink-0" />
        <span class="min-w-0 flex-1">{error}</span>
        <button type="button" class="text-[11px] underline" onclick={() => (error = null)}>Dismiss</button>
      </div>
    {/if}

    {#if current}
      {@const { entry, store } = current}
      <header class="flex items-center gap-3 border-b border-surface-200-800 px-5 py-3">
        <div class="min-w-0 flex-1">
          <div class="truncate text-sm font-medium">{entry.title}</div>
          <div class="truncate text-[11px] text-surface-500" title={entry.path}>
            {store.title}
            · {entry.relPath} · {formatBytes(entry.bytes ?? 0)}{entry.modifiedAt
              ? ` · ${formatAge(entry.modifiedAt)}`
              : ""}
          </div>
        </div>
        {#if editing}
          <button type="button" class="btn btn-sm preset-tonal" onclick={cancelEdit} disabled={saving}>Cancel</button>
          <button type="button" class="btn btn-sm preset-filled-primary-500" onclick={save} disabled={saving || !dirty}>
            {saving ? "Saving…" : "Save"}
          </button>
        {:else if confirmDelete}
          <span class="text-xs text-surface-600-400"
            >Delete this memory{store.entries.some((e) => e.kind === "index") && entry.kind !== "index"
              ? " and its index line"
              : ""}?</span
          >
          <button type="button" class="btn btn-sm preset-tonal" onclick={() => (confirmDelete = false)}>Keep</button>
          <button type="button" class="btn btn-sm preset-filled-error-500" onclick={remove}>Delete</button>
        {:else}
          <TooltipButton
            label="Edit"
            onclick={startEdit}
            aria-label="Edit memory"
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
            disabled={text === undefined}
          >
            <Pencil size={14} />
          </TooltipButton>
          <TooltipButton
            label="Reveal in Finder"
            onclick={() => commands.revealInFinder(entry.path)}
            aria-label="Reveal in Finder"
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
          >
            <FolderOpen size={14} />
          </TooltipButton>
          <TooltipButton
            label="Delete"
            onclick={() => (confirmDelete = true)}
            aria-label="Delete memory"
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
          >
            <Trash2 size={14} />
          </TooltipButton>
        {/if}
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto">
        {#if editing && draft !== undefined}
          <textarea
            class="h-full w-full resize-none bg-transparent px-5 py-4 font-mono text-xs leading-5 outline-none"
            aria-label="Memory contents"
            spellcheck="false"
            value={draft}
            oninput={(event) => (drafts[entry.path] = event.currentTarget.value)}
          ></textarea>
        {:else if text === undefined}
          <div class="grid h-full place-items-center">
            <RefreshCw size={16} class="animate-spin text-surface-500" />
          </div>
        {:else if isMarkdown(entry.path)}
          <div class="prose-md mx-auto max-w-3xl px-5 py-4 text-sm leading-7" use:copyCode>
            {@html renderMarkdown(stripFrontmatter(text))}
          </div>
        {:else}
          <pre class="px-5 py-4 font-mono text-xs leading-5 whitespace-pre-wrap">{text}</pre>
        {/if}
      </div>
    {:else if !loading}
      <div class="grid flex-1 place-items-center p-8">
        <div class="max-w-sm text-center">
          <Brain size={28} class="mx-auto text-surface-400" />
          <div class="mt-3 text-sm font-medium">{entryCount(homes) === 0 ? "No memories yet" : "Pick a memory"}</div>
          <p class="mt-1 text-xs leading-5 text-surface-500">
            {#if entryCount(homes) === 0}
              Codex and Claude Code keep notes between conversations. Turn memories on for a Home on the left, and they
              show up here once a harness writes some.
            {:else}
              Choose a memory on the left to read, edit or delete it.
            {/if}
          </p>
        </div>
      </div>
    {/if}
  </section>
</div>
