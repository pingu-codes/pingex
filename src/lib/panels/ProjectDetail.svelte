<script lang="ts">
import {
  AlertCircle,
  CheckCircle2,
  File as FileIcon,
  FolderOpen,
  FolderPlus,
  Loader,
  MessageSquare,
  Plus,
  RefreshCw,
  Search,
  Trash2,
  X,
} from "@lucide/svelte";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { untrack } from "svelte";
import { eventMatchesHome } from "$lib/app/launch.svelte";
import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  addProjectSource,
  isTauri,
  listProjectSources,
  reindexSource,
  removeProjectSource,
  revealInFinder,
  saveProjectInstructions,
  searchWorkspace,
} from "$lib/services/api";
import type { Project, ProjectSource, WorkspaceSearchResults } from "$lib/types";
import { relativeTime } from "$lib/utils/time";
import { debounce, emptyStateLabel, isEmptyResults } from "./workspaceSearch";

let {
  project,
  onOpenThread,
  onNewThread,
  onManageWorkspace,
}: {
  project: Project;
  onOpenThread: (threadId: string) => void;
  onNewThread?: (project: Project) => void;
  onManageWorkspace?: (project: Project) => void;
} = $props();

// The detail view is keyed by project path in the parent, so this component is
// recreated when a different project opens; capturing the initial prop values is
// intentional (and avoids resetting an in-progress edit when the project object
// is replaced by a background refresh).
let instructions = $state(untrack(() => project.instructions ?? ""));
let savingInstructions = $state(false);
let sources = $state<ProjectSource[]>(untrack(() => project.sources ?? []));
let sourceError = $state<string | null>(null);

let query = $state("");
let results = $state<WorkspaceSearchResults | null>(null);
let searching = $state(false);
// Monotonic token so a slow, stale response cannot overwrite a newer one.
let generation = 0;

// Background indexing (Rust) announces completion; refresh the source rows.
$effect(() => {
  if (!isTauri()) return;
  const unlisten = listen<{ projectPath: string; codexHome?: string }>("sources://updated", (event) => {
    if (!eventMatchesHome(event.payload?.codexHome)) return;
    if (event.payload?.projectPath === project.path) void refreshSources();
  });
  return () => {
    void unlisten.then((off) => off());
  };
});

async function refreshSources() {
  try {
    sources = await listProjectSources(project.path);
  } catch {
    // A failed refresh leaves the last-known rows in place.
  }
}

const saveInstructions = debounce((path: string, text: string) => {
  savingInstructions = true;
  saveProjectInstructions(path, text)
    .catch(() => {})
    .finally(() => {
      savingInstructions = false;
    });
}, 400);

function onInstructionsInput(event: Event) {
  instructions = (event.target as HTMLTextAreaElement).value;
  saveInstructions(project.path, instructions);
}

async function attach(path: string, kind: "folder" | "file") {
  try {
    sources = await addProjectSource(project.path, path, kind);
  } catch (cause) {
    sourceError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function addFolder() {
  sourceError = null;
  if (!isTauri()) {
    await attach(`${project.path}/preview-folder`, "folder");
    return;
  }
  const selection = await open({ directory: true, multiple: true, title: "Add source folder" });
  if (!selection) return;
  for (const path of Array.isArray(selection) ? selection : [selection]) {
    await attach(path, "folder");
  }
}

async function addFiles() {
  sourceError = null;
  if (!isTauri()) {
    await attach(`${project.path}/preview-file.md`, "file");
    return;
  }
  const selection = await open({ directory: false, multiple: true, title: "Add source files" });
  if (!selection) return;
  for (const path of Array.isArray(selection) ? selection : [selection]) {
    await attach(path, "file");
  }
}

async function removeSource(id: string) {
  try {
    sources = await removeProjectSource(id, project.path);
  } catch (cause) {
    sourceError = cause instanceof Error ? cause.message : String(cause);
  }
}

async function retrySource(id: string) {
  await reindexSource(id).catch(() => {});
  await refreshSources();
}

const runSearch = debounce((value: string) => {
  const trimmed = value.trim();
  if (!trimmed) {
    results = null;
    searching = false;
    return;
  }
  const token = ++generation;
  searching = true;
  searchWorkspace(project.path, trimmed, null, token)
    .then((response) => {
      // Drop stale responses (a newer query has since been issued).
      if (response.generation === generation) results = response;
    })
    .catch(() => {
      if (token === generation) results = null;
    })
    .finally(() => {
      if (token === generation) searching = false;
    });
}, 200);

function onQueryInput(event: Event) {
  query = (event.target as HTMLInputElement).value;
  runSearch(query);
}

function fileTitle(path: string, line?: number | null): string {
  return line ? `${project.path}/${path}:${line}` : `${project.path}/${path}`;
}

function openFile(path: string) {
  void revealInFinder(`${project.path}/${path}`);
}

const sourceName = (source: ProjectSource) => source.sourcePath.split("/").pop() || source.sourcePath;
const isMultiProject = $derived(project.kind === "multiProject");
</script>

<div class="h-full overflow-y-auto">
  <div class="mx-auto max-w-3xl px-6 py-8">
    <!-- Detail header -->
    <div class="flex items-start justify-between gap-4">
      <div class="min-w-0">
        <h1 class="truncate text-lg font-semibold tracking-[-0.02em]" title={project.name}>{project.name}</h1>
        <p class="mt-0.5 truncate font-mono text-xs text-surface-500" title={project.path}>{project.path}</p>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        {#if onNewThread}
          <button onclick={() => onNewThread?.(project)} class="btn btn-sm preset-tonal">
            <Plus size={14} />
            New thread
          </button>
        {/if}
        <button onclick={addFiles} class="btn btn-sm preset-tonal">
          <FileIcon size={14} />
          Add files
        </button>
        <button onclick={addFolder} class="btn btn-sm preset-filled-primary-500">
          <FolderPlus size={14} />
          Add source
        </button>
      </div>
    </div>

    {#if isMultiProject}
      <section class="mt-5 rounded-xl border border-surface-200-800 bg-surface-100-900 p-4">
        <div class="flex items-center justify-between gap-3"><h2 class="text-sm font-medium">Workspace members</h2>{#if onManageWorkspace}<button class="btn btn-sm preset-tonal" onclick={() => onManageWorkspace?.(project)}>Edit members</button>{/if}</div>
        <p class="mt-1 text-xs text-surface-500">The hub is writable for notes and plans. Member aliases are the directories the agent edits.</p>
        <div class="mt-3 space-y-2">
          {#each project.members ?? [] as member (member.alias)}
            <div class="flex min-w-0 items-center gap-3 rounded-lg bg-surface-50-950 px-3 py-2 text-xs">
              <span class="font-mono font-medium text-primary-600 dark:text-primary-400">{member.alias}</span>
              <span class="min-w-0 flex-1 truncate text-surface-500" title={member.effectivePath}>{member.effectivePath}</span>
              <span class="shrink-0 rounded-full bg-surface-200-800 px-2 py-0.5 text-[10px]">{member.isolated ? `isolated${member.branch ? ` · ${member.branch}` : ""}` : "direct"}</span>
              {#if !member.available}<span class="shrink-0 text-error-500">unavailable</span>{/if}
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <!-- Search -->
    <div class="mt-6">
      <div class="flex items-center gap-2 rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2">
        <Search size={15} class="shrink-0 text-surface-500" />
        <input
          value={query}
          oninput={onQueryInput}
          type="search"
          placeholder="Search project files and chats…"
          aria-label="Search workspace"
          class="min-w-0 flex-1 bg-transparent text-sm outline-none placeholder:text-surface-500"
        />
        {#if searching}<Loader size={14} class="shrink-0 animate-spin text-surface-500" />{/if}
      </div>

      {#if query.trim() && results}
        {#if isEmptyResults(results)}
          <div class="mt-3 rounded-xl border border-dashed border-surface-300-700 px-4 py-8 text-center text-sm text-surface-500">
            {emptyStateLabel(query)}
          </div>
        {:else}
          <div class="mt-3 space-y-4">
            {#if results.projectFiles.items.length > 0}
              <div>
                <h3 class="mb-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Project files</h3>
                <div class="space-y-1">
                  {#each results.projectFiles.items as match (match.path + ":" + (match.lineNumber ?? "name"))}
                    <button
                      onclick={() => openFile(match.path)}
                      title={fileTitle(match.path, match.lineNumber)}
                      class="flex w-full items-center gap-2.5 rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-left hover:preset-tonal"
                    >
                      <FileIcon size={14} class="shrink-0 text-surface-500" />
                      <div class="min-w-0 flex-1">
                        <div class="flex items-baseline gap-1.5">
                          <span class="truncate text-xs font-medium">{match.fileName}</span>
                          <span class="truncate text-[10px] text-surface-500">{match.path}{match.lineNumber ? `:${match.lineNumber}` : ""}</span>
                        </div>
                        {#if match.preview}
                          <div class="truncate font-mono text-[11px] text-surface-600-400">{match.preview}</div>
                        {/if}
                      </div>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}

            {#if results.threads.items.length > 0}
              <div>
                <h3 class="mb-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Local chats</h3>
                <div class="space-y-1">
                  {#each results.threads.items as match (match.threadId)}
                    <button
                      onclick={() => onOpenThread(match.threadId)}
                      class="flex w-full items-center gap-2.5 rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-left hover:preset-tonal"
                    >
                      <MessageSquare size={14} class="shrink-0 text-surface-500" />
                      <span class="min-w-0 flex-1 truncate text-xs">{match.title}</span>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}

            {#if results.messages.items.length > 0}
              <div>
                <h3 class="mb-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Thread messages</h3>
                <div class="space-y-1">
                  {#each results.messages.items as match (match.threadId)}
                    <button
                      onclick={() => onOpenThread(match.threadId)}
                      class="flex w-full items-center gap-2.5 rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-left hover:preset-tonal"
                    >
                      <MessageSquare size={14} class="shrink-0 text-surface-500" />
                      <span class="min-w-0 flex-1 truncate text-xs">{match.title}</span>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          </div>
        {/if}
      {/if}
    </div>

    <!-- Instructions -->
    <div class="mt-6">
      <div class="mb-1.5 flex items-center gap-2">
        <h2 class="text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Instructions</h2>
        {#if savingInstructions}<span class="text-[10px] text-surface-500">Saving…</span>{/if}
      </div>
      <textarea
        value={instructions}
        oninput={onInstructionsInput}
        rows="4"
        placeholder="Context Codex should keep in mind for every thread in this project…"
        aria-label="Project instructions"
        class="w-full resize-y rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-sm outline-none placeholder:text-surface-500 focus:border-primary-500"
      ></textarea>
    </div>

    <!-- Sources -->
    <div class="mt-6">
      <h2 class="mb-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Sources</h2>
      {#if sourceError}
        <div class="mb-2 flex items-center gap-2 rounded-lg preset-tonal-error px-3 py-2 text-xs">
          <span class="min-w-0 flex-1">{sourceError}</span>
          <TooltipButton label="Dismiss" onclick={() => (sourceError = null)} aria-label="Dismiss" class="btn-icon btn-icon-sm"><X size={12} /></TooltipButton>
        </div>
      {/if}
      {#if sources.length === 0}
        <button
          onclick={addFolder}
          class="w-full rounded-xl border border-dashed border-surface-300-700 px-4 py-6 text-left text-xs leading-5 text-surface-600-400 hover:border-surface-400-600 hover:text-surface-800-200"
        >
          Attach folders or files to make their contents searchable in this project.
        </button>
      {:else}
        <div class="space-y-1.5">
          {#each sources as source (source.id)}
            <div class="group/src flex items-center gap-3 rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2.5">
              {#if source.kind === "folder"}
                <FolderOpen size={16} strokeWidth={1.7} class="shrink-0 text-surface-500" />
              {:else}
                <FileIcon size={16} strokeWidth={1.7} class="shrink-0 text-surface-500" />
              {/if}
              <div class="min-w-0 flex-1">
                <div class="truncate text-sm font-medium" title={source.sourcePath}>{sourceName(source)}</div>
                <div class="truncate text-[11px] text-surface-500">{source.sourcePath}</div>
              </div>
              <!-- Status badge -->
              {#if source.status === "pending"}
                <span class="flex shrink-0 items-center gap-1 text-[11px] text-surface-500">
                  <Loader size={12} class="animate-spin" />
                  Indexing…
                </span>
              {:else if source.status === "indexed"}
                <TooltipAnchor label={source.indexedAt ? `Indexed ${relativeTime(source.indexedAt)}` : "Indexed"} class="flex shrink-0 items-center gap-1 text-[11px] text-success-600 dark:text-success-400">
                  <CheckCircle2 size={12} />
                  {source.docCount} {source.docCount === 1 ? "file" : "files"}
                </TooltipAnchor>
              {:else}
                <TooltipButton
                  label={source.error ?? "Indexing failed"}
                  onclick={() => retrySource(source.id)}
                  class="flex shrink-0 items-center gap-1 text-[11px] text-error-500 hover:underline"
                >
                  <AlertCircle size={12} />
                  Retry
                  <RefreshCw size={11} />
                </TooltipButton>
              {/if}
              <TooltipButton
                label="Remove source"
                onclick={() => removeSource(source.id)}
                aria-label="Remove source"
                class="btn-icon btn-icon-sm shrink-0 text-surface-500 opacity-0 transition hover:text-error-500 focus:opacity-100 group-hover/src:opacity-100"
              >
                <Trash2 size={14} />
              </TooltipButton>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
</div>
