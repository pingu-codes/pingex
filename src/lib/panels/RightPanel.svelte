<script lang="ts">
import {
  ArrowLeft,
  Eraser,
  FileDiff,
  FolderTree,
  Gauge,
  Globe,
  Hammer,
  Lightbulb,
  MessageCircleQuestion,
  ScrollText,
  SquareTerminal,
  X,
} from "@lucide/svelte";
import DiffBlock from "$lib/components/DiffBlock.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import MessageLog from "$lib/layout/MessageLog.svelte";
import FileTree from "$lib/panels/FileTree.svelte";
import ProcessDetail from "$lib/panels/ProcessDetail.svelte";
import SideQuestions from "$lib/panels/SideQuestions.svelte";
import ThreadStatus from "$lib/panels/ThreadStatus.svelte";
import { revealInFinder } from "$lib/services/api";
import { processByKey, type RunningProcess } from "$lib/services/processes.svelte";
import type { ContextStats } from "$lib/thread/contextUsage";
import type { BootstrapData, FileUpdateChange, SideQuestion } from "$lib/types";
import { renderMarkdown } from "$lib/utils/markdown";
import { loadSize, resizeHandle } from "$lib/utils/resize";

export type PanelView =
  | { kind: "plan"; text: string }
  | { kind: "sources"; queries: string[] }
  | { kind: "side" }
  | { kind: "diffs"; focusPath?: string | null }
  | { kind: "files" }
  | { kind: "messageLog" }
  | { kind: "status" }
  | { kind: "process"; processKey: string };

let {
  view,
  parentThreadId,
  sideQuestions,
  changes = [],
  cwd = "",
  contextStats = null,
  costUsd = null,
  activeModel = null,
  onClose,
  onDataChanged,
  onImplementPlan,
  onImplementPlanFresh,
  implementDisabled = false,
  onStopProcessTurn,
}: {
  view: PanelView;
  parentThreadId: string | null;
  sideQuestions: SideQuestion[];
  changes?: FileUpdateChange[];
  cwd?: string;
  /** Context usage for the `status` view. */
  contextStats?: ContextStats | null;
  costUsd?: number | null;
  activeModel?: string | null;
  onClose: () => void;
  onDataChanged: (data: BootstrapData) => void;
  onImplementPlan?: () => void;
  /** Interrupt the turn a running process belongs to; only for the open thread. */
  onStopProcessTurn?: (process: RunningProcess) => void;
  /** Implement the plan in a fresh thread; absent when there is no live thread. */
  onImplementPlanFresh?: () => void;
  implementDisabled?: boolean;
} = $props();

let activeSideId = $state<string | null>(null);
let panelWidth = $state(loadSize("layout.rightPanelWidth", 380, 280, 720));

let body = $state<HTMLElement | null>(null);

// Scroll the focused file's diff into view when the diff panel opens on a
// specific output.
$effect(() => {
  if (view.kind !== "diffs" || !view.focusPath || !body) return;
  const target = body.querySelector(`[data-diff-path="${CSS.escape(view.focusPath)}"]`);
  target?.scrollIntoView?.({ block: "start" });
});

function openProjectFile(relativePath: string) {
  const absolute = `${cwd.replace(/\/$/, "")}/${relativePath}`;
  revealInFinder(absolute).catch(() => {});
}
</script>

<aside
  class="relative flex h-full shrink-0 flex-col border-l border-surface-200-800 bg-surface-100-900"
  style="width: {panelWidth}px"
  aria-label="Thread side panel"
>
  <div
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize panel"
    class="absolute inset-y-0 -left-1 z-20 w-2 cursor-col-resize transition-colors hover:bg-primary-500/30 active:bg-primary-500/40"
    use:resizeHandle={{
      axis: "x",
      direction: -1,
      min: 280,
      max: 720,
      storageKey: "layout.rightPanelWidth",
      getSize: () => panelWidth,
      onResize: (size) => (panelWidth = size),
    }}
  ></div>
  <header class="flex h-11 shrink-0 items-center gap-2 border-b border-surface-200-800 px-3">
    {#if view.kind === "side" && activeSideId}
      <TooltipButton label="Back to side questions" aria-label="Back to side questions" onclick={() => (activeSideId = null)} class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
        <ArrowLeft size={14} />
      </TooltipButton>
    {/if}
    {#if view.kind === "plan"}
      <Lightbulb size={14} class="text-warning-500" />
      <span class="flex-1 text-xs font-semibold">Plan</span>
    {:else if view.kind === "sources"}
      <Globe size={14} class="text-surface-500" />
      <span class="flex-1 text-xs font-semibold">Sources</span>
    {:else if view.kind === "diffs"}
      <FileDiff size={14} class="text-surface-500" />
      <span class="flex-1 text-xs font-semibold">Changes</span>
      {#if changes.length > 0}
        <span class="text-[10px] text-surface-500">{changes.length}</span>
      {/if}
    {:else if view.kind === "files"}
      <FolderTree size={14} class="text-surface-500" />
      <span class="flex-1 text-xs font-semibold">Files</span>
    {:else if view.kind === "messageLog"}
      <ScrollText size={14} class="text-surface-500" />
      <span class="flex-1 text-xs font-semibold">Message log</span>
    {:else if view.kind === "status"}
      <Gauge size={14} class="text-surface-500" />
      <span class="flex-1 text-xs font-semibold">Status</span>
    {:else if view.kind === "process"}
      <SquareTerminal size={14} class="text-surface-500" />
      <span class="flex-1 text-xs font-semibold">Process</span>
    {:else}
      <MessageCircleQuestion size={14} class="text-primary-500" />
      <span class="flex-1 text-xs font-semibold">Side questions</span>
    {/if}
    <TooltipButton label="Close panel" aria-label="Close panel" onclick={onClose} class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
      <X size={14} />
    </TooltipButton>
  </header>

  {#if view.kind === "side"}
    <SideQuestions {parentThreadId} {sideQuestions} bind:activeSideId {onDataChanged} />
  {:else}
    <div class="min-h-0 flex-1 overflow-y-auto p-3" bind:this={body}>
      {#if view.kind === "diffs"}
        {#if changes.length === 0}
          <p class="text-xs text-surface-500">No file changes in this thread.</p>
        {:else}
          <div class="space-y-3">
            {#each changes as change (change.path)}
              <div data-diff-path={change.path}>
                <DiffBlock {change} />
              </div>
            {/each}
          </div>
        {/if}
      {:else if view.kind === "files"}
        <FileTree root={cwd} onOpenFile={openProjectFile} />
      {:else if view.kind === "plan"}
        <div class="prose-side text-xs leading-6">
          {@html renderMarkdown(view.text)}
        </div>
      {:else if view.kind === "sources"}
        {#if view.queries.length === 0}
          <p class="text-xs text-surface-500">No web searches in this thread.</p>
        {:else}
          <ul class="space-y-2">
            {#each view.queries as query, index (index)}
              <li class="flex items-start gap-2 rounded-lg bg-surface-50-950 px-2.5 py-2 text-xs leading-5">
                <Globe size={12} class="mt-0.5 shrink-0 text-surface-500" />
                <span class="min-w-0 break-words">{query}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if view.kind === "messageLog"}
        <MessageLog />
      {:else if view.kind === "status"}
        <ThreadStatus stats={contextStats} {costUsd} model={activeModel} threadId={parentThreadId} />
      {:else if view.kind === "process"}
        {@const process = processByKey(view.processKey)}
        <!-- Interrupting is only possible for the open thread's own turn. -->
        <ProcessDetail
          {process}
          onStopTurn={process?.threadId === parentThreadId ? onStopProcessTurn : undefined}
        />
      {/if}
    </div>
    {#if view.kind === "plan" && onImplementPlan}
      <footer class="shrink-0 space-y-2 border-t border-surface-200-800 p-3">
        <button
          class="btn btn-sm preset-filled-primary-500 w-full"
          disabled={implementDisabled}
          onclick={onImplementPlan}
        >
          <Hammer size={14} />
          Implement plan
        </button>
        {#if onImplementPlanFresh}
          <TooltipButton
            label="Start a new thread whose only context is this plan"
            aria-label="Clear context and implement the plan"
            class="btn btn-sm preset-tonal w-full"
            disabled={implementDisabled}
            onclick={onImplementPlanFresh}
          >
            <Eraser size={14} />
            Clear context & implement
          </TooltipButton>
        {/if}
      </footer>
    {/if}
  {/if}
</aside>

<style>
  .prose-side :global(p) {
    margin: 0.35rem 0;
  }
  .prose-side :global(ul),
  .prose-side :global(ol) {
    margin: 0.35rem 0;
    padding-left: 1.1rem;
  }
  .prose-side :global(ul) {
    list-style: disc;
  }
  .prose-side :global(.table-wrap) {
    margin: 0.5rem 0;
    overflow-x: auto;
  }
  .prose-side :global(table) {
    width: 100%;
    border-collapse: collapse;
    line-height: 1.45;
  }
  .prose-side :global(th),
  .prose-side :global(td) {
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    padding: 0.25rem 0.5rem;
    text-align: left;
    vertical-align: top;
  }
  .prose-side :global(th) {
    background: color-mix(in oklab, currentColor 6%, transparent);
    font-weight: 600;
  }
  .prose-side :global(pre) {
    overflow-x: auto;
    border-radius: 0.5rem;
    background: #0d1117;
    padding: 0.5rem 0.7rem;
    font-size: 11px;
  }
</style>
