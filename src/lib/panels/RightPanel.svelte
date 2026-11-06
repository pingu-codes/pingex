<script lang="ts">
  import {
    AppWindow,
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
  import { openFilePreview } from "$lib/app/filePreview";
  import BrowserPanel from "$lib/browser/BrowserPanel.svelte";
  import DiffBlock from "$lib/components/DiffBlock.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import MessageLog from "$lib/layout/MessageLog.svelte";
  import ChangesPanel from "$lib/panels/ChangesPanel.svelte";
  import FileTree from "$lib/panels/FileTree.svelte";
  import ProcessDetail from "$lib/panels/ProcessDetail.svelte";
  import SideQuestions from "$lib/panels/SideQuestions.svelte";
  import ThreadStatus from "$lib/panels/ThreadStatus.svelte";
  import { processByKey, type RunningProcess } from "$lib/services/processes.svelte";
  import type { ContextStats } from "$lib/thread/contextUsage";
  import type { BootstrapData, ChangesSummary, FileUpdateChange, SideQuestion } from "$lib/types";
  import { renderMarkdown } from "$lib/utils/markdown";
  import { resolveMentionPath } from "$lib/utils/mentions";
  import { loadSize, resizeHandle } from "$lib/utils/resize";

  export type PanelView =
    | { kind: "plan"; text: string }
    | { kind: "sources"; queries: string[] }
    | { kind: "side"; submission?: { id: number; text: string } }
    | { kind: "diffs"; focusPath?: string | null }
    | { kind: "changes"; focusPath?: string | null }
    | { kind: "files" }
    | { kind: "messageLog" }
    | { kind: "status" }
    | { kind: "browser" }
    | { kind: "process"; processKey: string };

  let {
    view,
    parentThreadId,
    sideQuestions,
    changes = [],
    gitChanges = null,
    gitChangesLoading = false,
    gitChangesError = null,
    onRefreshGitChanges = () => {},
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
    showSystemPromptInThread = false,
    onToggleSystemPromptInThread,
  }: {
    view: PanelView;
    parentThreadId: string | null;
    sideQuestions: SideQuestion[];
    changes?: FileUpdateChange[];
    /** Git-derived changes for the `changes` view (summary only; diffs load lazily). */
    gitChanges?: ChangesSummary | null;
    gitChangesLoading?: boolean;
    gitChangesError?: string | null;
    onRefreshGitChanges?: () => void;
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
    /** Whether the system prompt's full text is currently shown inline in the thread. */
    showSystemPromptInThread?: boolean;
    /** Fetches (once) and toggles the full text in the thread. Omitted when
     *  the thread's harness cannot supply it. */
    onToggleSystemPromptInThread?: () => void;
  } = $props();

  const PANEL_META: Record<PanelView["kind"], { title: string; icon: typeof X; tone?: string }> = {
    plan: { title: "Plan", icon: Lightbulb, tone: "text-warning-500" },
    sources: { title: "Sources", icon: Globe },
    side: { title: "Side questions", icon: MessageCircleQuestion, tone: "text-primary-500" },
    diffs: { title: "Outputs", icon: FileDiff },
    changes: { title: "Changes", icon: FileDiff },
    files: { title: "Files", icon: FolderTree },
    messageLog: { title: "Message log", icon: ScrollText },
    status: { title: "Status", icon: Gauge },
    browser: { title: "Browser", icon: AppWindow },
    process: { title: "Process", icon: SquareTerminal },
  };
  const panelMeta = $derived(PANEL_META[view.kind]);
  const PanelIcon = $derived(panelMeta.icon);

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
    void openFilePreview({ path: resolveMentionPath(relativePath, cwd), line: null, column: null }, cwd);
  }
</script>

<!--
  The panel owns Escape. The main composer interrupts the running turn on any
  Escape that reaches the window unclaimed, and a side question's Stop uses the
  same key — so an Escape pressed here (typing a side question, or after its
  answer landed) must never fall through and stop the main thread.
-->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -- claiming a key, not making the panel interactive -->
<aside
  class="relative flex h-full shrink-0 flex-col border-l border-surface-200-800 bg-surface-100-900"
  style="width: {panelWidth}px; max-width: max(0px, calc(100% - 280px))"
  aria-label="Thread side panel"
  onkeydown={(event) => {
    if (event.key === "Escape") event.preventDefault();
  }}
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
      <TooltipButton
        label="Back to side questions"
        aria-label="Back to side questions"
        onclick={() => (activeSideId = null)}
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      >
        <ArrowLeft size={14} />
      </TooltipButton>
    {/if}
    <PanelIcon size={14} class={panelMeta.tone ?? "text-surface-500"} />
    <span class="flex-1 text-xs font-semibold">{panelMeta.title}</span>
    {#if view.kind === "diffs" && changes.length > 0}
      <span class="text-[10px] text-surface-500">{changes.length}</span>
    {/if}
    <TooltipButton
      label="Close panel"
      aria-label="Close panel"
      onclick={onClose}
      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
    >
      <X size={14} />
    </TooltipButton>
  </header>

  {#if view.kind === "side"}
    <SideQuestions {parentThreadId} {sideQuestions} bind:activeSideId {onDataChanged} submission={view.submission} />
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
      {:else if view.kind === "changes"}
        <ChangesPanel
          {cwd}
          summary={gitChanges}
          loading={gitChangesLoading}
          error={gitChangesError}
          focusPath={view.focusPath ?? null}
          onRefresh={onRefreshGitChanges}
        />
      {:else if view.kind === "files"}
        <FileTree root={cwd} onOpenFile={openProjectFile} />
      {:else if view.kind === "plan"}
        <div class="prose-md prose-md-compact text-xs leading-6">
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
        <ThreadStatus
          stats={contextStats}
          {costUsd}
          model={activeModel}
          threadId={parentThreadId}
          {showSystemPromptInThread}
          {onToggleSystemPromptInThread}
        />
      {:else if view.kind === "browser"}
        <BrowserPanel threadId={parentThreadId} />
      {:else if view.kind === "process"}
        {@const process = processByKey(view.processKey)}
        <!-- Interrupting is only possible for the open thread's own turn. -->
        <ProcessDetail {process} onStopTurn={process?.threadId === parentThreadId ? onStopProcessTurn : undefined} />
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
