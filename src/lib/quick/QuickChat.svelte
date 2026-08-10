<script lang="ts">
import { ArrowUpRight, ChevronDown, Folder, Send } from "@lucide/svelte";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import ModelPopover from "$lib/composer/ModelPopover.svelte";
import { applyQuickEvent, emptyQuickResponse, type QuickResponse } from "$lib/quick/quickChat";
import { bootstrap, isTauri, listModels, quickOpenFullThread, startThread, startTurn } from "$lib/services/api";
import { type CodexEvent, setThreadHandler, startCodexListeners } from "$lib/services/codexEvents.svelte";
import { requestAutoName } from "$lib/thread/autoName";
import type { Model, Project } from "$lib/types";
import { dragRegion } from "$lib/utils/dragRegion";

let projects = $state<Project[]>([]);
let models = $state<Model[] | null>(null);
let modelsError = $state<string | null>(null);
let selectedPath = $state<string | null>(null);
let modelId = $state<string | null>(null);
let effort = $state<string | null>(null);
let text = $state("");
let sending = $state(false);
let error = $state<string | null>(null);
let threadId = $state<string | null>(null);
let response = $state<QuickResponse>(emptyQuickResponse());
let projectMenuOpen = $state(false);
let modelMenuOpen = $state(false);
let inputEl = $state<HTMLTextAreaElement | null>(null);

const selectedProject = $derived(projects.find((project) => project.path === selectedPath) ?? null);
const selectedModel = $derived((models ?? []).find((model) => model.id === modelId) ?? null);
const effortOptions = $derived(selectedModel?.supportedReasoningEfforts ?? []);
const modelLabel = $derived(selectedModel?.displayName ?? "Model");

async function prefetch() {
  try {
    const data = await bootstrap();
    projects = data.projects.filter((project) => !project.archived);
    if (!selectedPath || !projects.some((project) => project.path === selectedPath)) {
      selectedPath = projects[0]?.path ?? null;
    }
  } catch (cause) {
    error = cause instanceof Error ? cause.message : String(cause);
  }
  try {
    const list = await listModels();
    models = list;
    if (!modelId) {
      const preferred = list.find((model) => model.isDefault) ?? list[0];
      modelId = preferred?.id ?? null;
      effort = preferred?.defaultReasoningEffort ?? null;
    }
  } catch (cause) {
    modelsError = cause instanceof Error ? cause.message : String(cause);
  }
}

function chooseModel(model: Model) {
  modelId = model.id;
  effort = model.defaultReasoningEffort;
  modelMenuOpen = false;
}

function reset() {
  threadId = null;
  response = emptyQuickResponse();
  error = null;
}

function focusInput() {
  queueMicrotask(() => inputEl?.focus());
}

async function hideWindow() {
  if (!isTauri()) return;
  try {
    await getCurrentWindow().hide();
  } catch {
    // Best-effort: closing the panel should never surface an error.
  }
}

async function send() {
  const trimmed = text.trim();
  if (!trimmed || sending || !selectedPath) return;
  sending = true;
  error = null;
  try {
    const started = await startThread(selectedPath);
    threadId = started.id;
    response = emptyQuickResponse(started.id);
    await startTurn(
      started.id,
      [{ type: "text", text: trimmed }],
      modelId ? { model: modelId, effort: effort ?? undefined } : undefined,
    );
    requestAutoName(started.id, "seed", trimmed);
    text = "";
  } catch (cause) {
    error = cause instanceof Error ? cause.message : String(cause);
    response = emptyQuickResponse();
    threadId = null;
  } finally {
    sending = false;
  }
}

async function openFull() {
  if (!threadId) return;
  await quickOpenFullThread(threadId);
  reset();
  text = "";
}

function onInputKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    hideWindow();
    return;
  }
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    send();
  }
}

const unsubscribe = setThreadHandler((event: CodexEvent) => {
  const before = response;
  response = applyQuickEvent(response, event);
  // A quick-chat thread lands in the sidebar like any other, so it gets the
  // same second naming pass once its one turn is done.
  if (before.streaming && !response.streaming && response.threadId && !response.error) {
    requestAutoName(response.threadId, "reply");
  }
});

startCodexListeners();
prefetch();
focusInput();

// The window is hidden (not destroyed) between activations, so refresh
// projects/models and refocus each time the shortcut brings it forward.
let unlistenShown: (() => void) | null = null;
if (isTauri()) {
  listen("quickchat://shown", () => {
    prefetch();
    focusInput();
  }).then((off) => {
    unlistenShown = off;
  });
}

$effect(() => () => {
  unsubscribe();
  unlistenShown?.();
});
</script>

<div class="flex h-screen flex-col overflow-hidden bg-surface-50-950 text-surface-950-50">
  <header
    class="flex h-9 shrink-0 items-center justify-between gap-2 border-b border-surface-200-800 px-3 select-none"
    data-tauri-drag-region
    use:dragRegion
  >
    <span class="text-[11px] font-medium text-surface-500">Quick chat</span>
    <div class="relative">
      <button
        onclick={(event) => {
          event.stopPropagation();
          projectMenuOpen = !projectMenuOpen;
          modelMenuOpen = false;
        }}
        class="inline-flex max-w-[220px] items-center gap-1.5 rounded-full px-2 py-1 text-[11px] text-surface-500 transition hover:bg-surface-200-800 hover:text-surface-800-200"
      >
        <Folder size={12} class="shrink-0" />
        <span class="truncate">{selectedProject?.name ?? "Select project"}</span>
        <ChevronDown size={11} class="shrink-0" />
      </button>
      {#if projectMenuOpen}
        <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
        <div
          class="card absolute right-0 top-8 z-50 max-h-56 w-[240px] overflow-y-auto border border-surface-200-800 bg-surface-50-950 p-1 shadow-xl"
          onclick={(event) => event.stopPropagation()}
        >
          {#if projects.length === 0}
            <p class="px-2 py-2 text-xs text-surface-500">No projects yet.</p>
          {:else}
            {#each projects as project (project.path)}
              <button
                onclick={() => {
                  selectedPath = project.path;
                  projectMenuOpen = false;
                  focusInput();
                }}
                class="flex w-full flex-col rounded px-2 py-1.5 text-left hover:preset-tonal {project.path === selectedPath ? 'preset-tonal' : ''}"
              >
                <span class="truncate text-xs">{project.name}</span>
                <span class="truncate text-[10px] text-surface-500">{project.path}</span>
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  </header>

  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
  <div
    class="flex min-h-0 flex-1 flex-col p-3"
    onclick={() => {
      projectMenuOpen = false;
      modelMenuOpen = false;
    }}
  >
    <div class="relative flex-1">
      <textarea
        bind:this={inputEl}
        bind:value={text}
        onkeydown={onInputKeydown}
        placeholder="Ask a quick question…"
        class="input h-full w-full resize-none rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-sm outline-none focus:border-primary-500"
      ></textarea>
    </div>

    {#if error}
      <div class="mt-2 card preset-tonal-error px-2 py-1 text-[11px]">{error}</div>
    {/if}

    {#if response.text || response.streaming || response.error}
      <div class="mt-2 max-h-28 overflow-y-auto rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-xs leading-5">
        {#if response.error}
          <span class="text-error-500">{response.error}</span>
        {:else}
          <span class="whitespace-pre-wrap">{response.text}</span>{#if response.streaming}<span class="animate-pulse text-surface-500">▍</span>{/if}
        {/if}
      </div>
    {/if}

    <div class="mt-2 flex shrink-0 items-center justify-between gap-2">
      <ModelPopover
        open={modelMenuOpen}
        {models}
        {modelsError}
        {modelId}
        {effort}
        {effortOptions}
        label={modelLabel}
        onToggle={() => {
          modelMenuOpen = !modelMenuOpen;
          projectMenuOpen = false;
        }}
        onChooseModel={chooseModel}
        onChooseEffort={(value) => (effort = value)}
      />

      <div class="flex items-center gap-2">
        {#if threadId}
          <button
            onclick={openFull}
            class="btn btn-sm preset-tonal inline-flex items-center gap-1.5 text-xs"
          >
            <ArrowUpRight size={13} />
            Open full thread
          </button>
        {/if}
        <button
          onclick={send}
          disabled={!text.trim() || sending || !selectedPath}
          class="btn btn-sm preset-filled-primary-500 inline-flex items-center gap-1.5 text-xs disabled:opacity-50"
        >
          <Send size={13} />
          {sending ? "Sending…" : "Send"}
        </button>
      </div>
    </div>
  </div>
</div>
