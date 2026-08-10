<script lang="ts">
import {
  Bot,
  ChevronDown,
  Code2,
  FileDiff,
  FileText,
  Folder,
  FolderTree,
  Globe,
  Lightbulb,
  ListTree,
  MessageCircleQuestion,
  Plus,
  ScrollText,
  SquareTerminal,
} from "@lucide/svelte";
import { messageLog } from "$lib/layout/messageLogPrefs.svelte";
import UsageMeter from "$lib/layout/UsageMeter.svelte";
import { accountUsage } from "$lib/services/accountUsage.svelte";
import { elapsedLabel } from "$lib/services/agentRuns.svelte";
import { processClock, type RunningProcess } from "$lib/services/processes.svelte";
import { type ContextStats, formatTokens, formatTokensShort } from "$lib/thread/contextUsage";
import { changeLabel } from "$lib/thread/fileChanges";
import { formatCost } from "$lib/thread/usageCost";
import type { FileUpdateChange, SubagentDetail } from "$lib/types";
import { fileIconFor } from "$lib/utils/fileIcons";

let {
  plan,
  outputs,
  sources,
  sideQuestionCount,
  subagents = [],
  processes = [],
  currentThreadId = null,
  contextStats = null,
  costUsd = null,
  onOpenFinder,
  onOpenZed,
  onShowPlan,
  onShowSources,
  onShowSideQuestions,
  onShowDiff,
  onShowFiles,
  onShowMessageLog,
  onOpenSubagent = () => {},
  onStopSubagent = () => {},
  onOpenProcess = () => {},
}: {
  plan: string | null;
  /** Every file the thread touched — created, edited, deleted alike. */
  outputs: FileUpdateChange[];
  sources: string[];
  sideQuestionCount: number;
  subagents?: SubagentDetail[];
  /** Commands Codex is running (or ran), across all threads. */
  processes?: RunningProcess[];
  /** Marks which processes belong to this thread vs another one. */
  currentThreadId?: string | null;
  contextStats?: ContextStats | null;
  /** Estimated API-equivalent spend for this thread, when a model is known. */
  costUsd?: number | null;
  onOpenFinder: () => void;
  onOpenZed: () => void;
  onShowPlan: () => void;
  onShowSources: () => void;
  onShowSideQuestions: () => void;
  onShowDiff: (path: string | null) => void;
  onShowFiles: () => void;
  onShowMessageLog: () => void;
  onOpenSubagent?: (agent: SubagentDetail) => void;
  /** Only ever called for app-owned agents, which are the only ones we can stop. */
  onStopSubagent?: (agent: SubagentDetail) => void;
  onOpenProcess?: (process: RunningProcess) => void;
} = $props();

let openIn = $state(false);
// The overview panel is persistent: it starts open and only the toggle
// button closes it, so opening plan/sources/side questions keeps it around.
let panel = $state(true);

const basename = (path: string) => path.split("/").pop() || path;
const cost = $derived(formatCost(costUsd));
// Named per-model buckets (e.g. Spark) sit alongside the account-wide limit.
const extraLimits = $derived(
  Object.values(accountUsage.byLimitId).filter(
    (bucket) => bucket.limitName && bucket.limitId !== accountUsage.snapshot?.limitId,
  ),
);
const subagentById = $derived(new Map(subagents.map((agent) => [agent.id, agent])));
const orderedSubagents = $derived.by(() => {
  const children = new Map<string, SubagentDetail[]>();
  for (const agent of subagents) {
    const group = children.get(agent.parentThreadId) ?? [];
    group.push(agent);
    children.set(agent.parentThreadId, group);
  }
  const ordered: SubagentDetail[] = [];
  const seen = new Set<string>();
  const visit = (agent: SubagentDetail) => {
    if (seen.has(agent.id)) return;
    seen.add(agent.id);
    ordered.push(agent);
    for (const child of children.get(agent.id) ?? []) visit(child);
  };
  for (const agent of subagents.filter((candidate) => !subagentById.has(candidate.parentThreadId))) visit(agent);
  for (const agent of subagents) visit(agent);
  return ordered;
});
const subagentDepth = (agent: SubagentDetail) => {
  let depth = 0;
  let parent = subagentById.get(agent.parentThreadId);
  const seen = new Set<string>();
  while (parent && !seen.has(parent.id)) {
    seen.add(parent.id);
    depth += 1;
    parent = subagentById.get(parent.parentThreadId);
  }
  return depth;
};
const subagentLabel = (agent: SubagentDetail) => agent.agentNickname ?? agent.agentRole ?? agent.title;
const subagentState = (status: string) => {
  if (["active", "running", "pendingInit", "inProgress"].includes(status)) return "Active";
  if (["errored", "failed", "systemError", "notFound"].includes(status)) return "Failed";
  if (["killed", "orphaned"].includes(status)) return "Stopped";
  return "Finished";
};

// Running first, then newest; the current thread's ahead of other threads'.
const orderedProcesses = $derived(
  [...processes].sort((a, b) => {
    const running = Number(b.status === "running") - Number(a.status === "running");
    if (running !== 0) return running;
    const local = Number(b.threadId === currentThreadId) - Number(a.threadId === currentThreadId);
    if (local !== 0) return local;
    return b.startedAt - a.startedAt;
  }),
);
const processState = (status: RunningProcess["status"]) =>
  status === "running" ? "Active" : status === "failed" ? "Failed" : status === "interrupted" ? "Stopped" : "Finished";

// Chips filter the process list; only Active is on by default so a long
// history of finished commands doesn't bury the ones still running.
const processFilterOptions = ["Active", "Finished", "Failed"] as const;
type ProcessFilter = (typeof processFilterOptions)[number];
let processFilters = $state<Record<ProcessFilter, boolean>>({ Active: true, Finished: false, Failed: false });
// Stopped (interrupted) commands are done running, so they live under Finished.
const processFilterBucket = (state: string): ProcessFilter =>
  state === "Active" ? "Active" : state === "Failed" ? "Failed" : "Finished";
const visibleProcesses = $derived(
  orderedProcesses.filter((process) => processFilters[processFilterBucket(processState(process.status))]),
);

const stateClass = (state: string) =>
  state === "Active" ? "text-success-500" : state === "Failed" ? "text-error-500" : "text-surface-500";

function closeDropdown() {
  openIn = false;
}
</script>

<svelte:window onclick={closeDropdown} onkeydown={(event) => event.key === "Escape" && closeDropdown()} />

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events, a11y_interactive_supports_focus -->
<div class="absolute right-4 top-2 z-30 select-none" onclick={(event) => event.stopPropagation()}>
  <!-- backdrop-blur makes this a stacking context, so it needs its own z-index to
       let the "Open in" dropdown paint above the overview panel below it. -->
  <div class="relative z-50 flex items-center gap-1 rounded-full border border-surface-200-800 bg-surface-50-950/95 p-1 shadow-md backdrop-blur">
    <div class="relative">
      <button
        onclick={() => {
          openIn = !openIn;
        }}
        class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs font-medium hover:preset-tonal"
      >
        Open in
        <ChevronDown size={12} class="text-surface-500" />
      </button>
      {#if openIn}
        <!-- z-50 keeps this above the overview panel, which renders later in the DOM. -->
        <div class="card absolute right-0 top-8 z-50 w-[160px] border border-surface-200-800 bg-surface-50-950 p-1 shadow-xl" role="menu">
          <button
            role="menuitem"
            onclick={() => {
              openIn = false;
              onOpenFinder();
            }}
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
          >
            <Folder size={13} class="text-surface-500" />
            Finder
          </button>
          <button
            role="menuitem"
            onclick={() => {
              openIn = false;
              onOpenZed();
            }}
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
          >
            <Code2 size={13} class="text-surface-500" />
            Zed
          </button>
        </div>
      {/if}
    </div>
    <button
      aria-label="Thread overview"
      onclick={() => {
        panel = !panel;
        openIn = false;
      }}
      class="btn-icon btn-icon-sm hover:preset-tonal text-surface-600-400"
    >
      <ListTree size={14} />
    </button>
  </div>

  {#if panel}
    <!-- max-height keeps the panel on-screen with lots of content; it scrolls instead. -->
    <div class="card absolute right-0 top-11 z-40 max-h-[calc(100vh-6rem)] w-[270px] overflow-y-auto border border-surface-200-800 bg-surface-50-950 p-2 shadow-xl" role="menu" aria-label="Thread overview panel">
      <div class="flex items-center px-1 pb-1 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">
        <span class="flex-1">Usage</span>
        {#if cost}<span class="normal-case tracking-normal">{cost} est.</span>{/if}
      </div>
      {#if contextStats}
        <dl class="space-y-1 px-2 pb-1.5 text-[11px] leading-4">
          <div class="flex justify-between gap-3">
            <dt class="text-surface-500">Context</dt>
            <dd class="font-mono">
              {formatTokensShort(contextStats.usedTokens)}{contextStats.contextWindow
                ? ` / ${formatTokensShort(contextStats.contextWindow)}`
                : ""}{contextStats.percentUsed !== null ? ` · ${contextStats.percentUsed}%` : ""}
            </dd>
          </div>
          <div class="flex justify-between gap-3">
            <dt class="text-surface-500">Thread tokens</dt>
            <dd class="font-mono">{formatTokens(contextStats.sessionTotalTokens)}</dd>
          </div>
          <div class="flex justify-between gap-3">
            <dt class="text-surface-500">In · cached</dt>
            <dd class="font-mono">
              {formatTokensShort(contextStats.sessionInputTokens)} · {formatTokensShort(contextStats.sessionCachedInputTokens)}
            </dd>
          </div>
          <div class="flex justify-between gap-3">
            <dt class="text-surface-500">Out · reasoning</dt>
            <dd class="font-mono">
              {formatTokensShort(contextStats.sessionOutputTokens)} · {formatTokensShort(contextStats.sessionReasoningTokens)}
            </dd>
          </div>
        </dl>
      {:else}
        <p class="px-2 py-1 text-xs text-surface-500">No tokens used yet.</p>
      {/if}
      <div class="space-y-2 px-2 pb-1 pt-1">
        <UsageMeter snapshot={accountUsage.snapshot} />
        {#each extraLimits as bucket (bucket.limitId)}
          <UsageMeter snapshot={bucket} namePrefix={bucket.limitName} />
        {/each}
      </div>

      <div class="mt-2 border-t border-surface-200-800 px-1 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Plan</div>
      {#if plan}
        <button
          onclick={onShowPlan}
          class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
        >
          <Lightbulb size={13} class="shrink-0 text-warning-500" />
          <span class="min-w-0 flex-1 truncate">{plan.split("\n")[0].replace(/^#+\s*/, "")}</span>
        </button>
      {:else}
        <p class="px-2 py-1 text-xs text-surface-500">No plan in this thread.</p>
      {/if}

      <div class="mt-2 flex items-center border-t border-surface-200-800 px-1 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">
        <span class="flex-1">Outputs</span>
        {#if outputs.length > 0}
          <span class="tabular-nums">{outputs.length}</span>
        {/if}
      </div>
      {#if outputs.length === 0}
        <p class="px-2 py-1 text-xs text-surface-500">No files changed yet.</p>
      {:else}
        <!-- Every touched file is listed, not just the first few: a file edited
             late in a long thread is exactly the one the user is looking for. -->
        <div class="max-h-56 overflow-y-auto">
          {#each outputs as change (change.path)}
            {@const icon = fileIconFor(basename(change.path))}
            <button
              onclick={() => onShowDiff(change.path)}
              title={`View diff for ${change.path}`}
              class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
            >
              <icon.icon size={13} class="shrink-0 {icon.class}" />
              <span class="min-w-0 flex-1 truncate">{basename(change.path)}</span>
              <span class="shrink-0 text-[10px] uppercase tracking-wide text-surface-500">
                {changeLabel(change.kind.type)}
              </span>
            </button>
          {/each}
        </div>
        <button
          onclick={() => onShowDiff(null)}
          class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs text-surface-500 hover:preset-tonal"
        >
          <FileDiff size={13} class="shrink-0" />
          <span class="min-w-0 flex-1 truncate">All {outputs.length} changed files</span>
        </button>
      {/if}

      <div class="mt-2 border-t border-surface-200-800 px-1 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">Sources</div>
      {#if sources.length === 0}
        <p class="px-2 py-1 text-xs text-surface-500">No web searches.</p>
      {:else}
        <button
          onclick={onShowSources}
          class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
        >
          <Globe size={13} class="shrink-0 text-surface-500" />
          <span class="min-w-0 flex-1 truncate">Web search</span>
          <span class="text-[10px] text-surface-500">{sources.length}</span>
        </button>
      {/if}

      <div class="mt-2 flex items-center border-t border-surface-200-800 px-1 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">
        <span class="flex-1">Subagents</span>
        {#if subagents.length > 0}<span>{subagents.length}</span>{/if}
      </div>
      {#if subagents.length === 0}
        <p class="px-2 py-1 text-xs text-surface-500">No subagents in this thread.</p>
      {:else}
        <div class="max-h-52 overflow-y-auto">
          {#each orderedSubagents as agent (agent.id)}
            {@const state = subagentState(agent.status)}
            <div class="flex w-full items-start rounded hover:preset-tonal">
              <button
                onclick={() => onOpenSubagent(agent)}
                style={`padding-left: ${8 + subagentDepth(agent) * 14}px`}
                class="flex min-w-0 flex-1 items-start gap-2 py-1.5 pr-2 text-left"
                aria-label={`Open subagent ${subagentLabel(agent)}`}
              >
                <Bot size={13} class="mt-0.5 shrink-0 {stateClass(state)}" />
                <span class="min-w-0 flex-1">
                  <span class="flex items-center gap-1.5">
                    <span class="min-w-0 flex-1 truncate text-xs">{subagentLabel(agent)}</span>
                    <span class="text-[9px] {stateClass(state)}">{state}</span>
                  </span>
                  <span class="mt-0.5 flex flex-wrap gap-1 text-[9px] text-surface-500">
                    <span>{agent.model ?? "Default model"}</span>
                    <span>·</span>
                    <span class="capitalize">{agent.reasoningEffort ?? "Default effort"}</span>
                  </span>
                </span>
              </button>
              <!-- Only app-owned agents have a process we can stop. -->
              {#if agent.source === "app" && agent.runId && state === "Active"}
                <button
                  onclick={() => onStopSubagent(agent)}
                  class="shrink-0 self-center px-2 py-1.5 text-[10px] text-surface-500 hover:text-error-500"
                  aria-label={`Stop subagent ${subagentLabel(agent)}`}
                >
                  Stop
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <div class="mt-2 flex items-center border-t border-surface-200-800 px-1 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-[0.08em] text-surface-500">
        <span class="flex-1">Processes</span>
        {#if processes.length > 0}<span>{processes.length}</span>{/if}
      </div>
      {#if processes.length === 0}
        <p class="px-2 py-1 text-xs text-surface-500">No commands run yet.</p>
      {:else}
        <div class="flex gap-1 px-1 pb-1.5">
          {#each processFilterOptions as filter (filter)}
            <button
              onclick={() => {
                processFilters[filter] = !processFilters[filter];
              }}
              aria-pressed={processFilters[filter]}
              class="rounded-full border px-2 py-0.5 text-[10px] font-medium transition-colors {processFilters[filter]
                ? 'border-primary-500/40 bg-primary-500/15 text-primary-500'
                : 'border-surface-200-800 text-surface-500 hover:preset-tonal'}"
            >
              {filter}
            </button>
          {/each}
        </div>
        {#if visibleProcesses.length === 0}
          <p class="px-2 py-1 text-xs text-surface-500">No matching processes.</p>
        {/if}
        <div class="max-h-52 overflow-y-auto">
          {#each visibleProcesses as process (process.key)}
            {@const state = processState(process.status)}
            <button
              onclick={() => onOpenProcess(process)}
              class="flex w-full items-start gap-2 rounded px-2 py-1.5 text-left hover:preset-tonal"
              aria-label={`Open process ${process.command}`}
            >
              <SquareTerminal size={13} class="mt-0.5 shrink-0 {stateClass(state)}" />
              <span class="min-w-0 flex-1">
                <span class="flex items-center gap-1.5">
                  <span class="min-w-0 flex-1 truncate font-mono text-xs">{process.command || "(command)"}</span>
                  <span class="text-[9px] {stateClass(state)}">{state}</span>
                </span>
                <span class="mt-0.5 flex flex-wrap gap-1 text-[9px] text-surface-500">
                  {#if process.status === "running"}
                    <span class="tabular-nums">{elapsedLabel(process.startedAt, processClock.now)}</span>
                  {:else if process.exitCode != null}
                    <span>exit {process.exitCode}</span>
                  {/if}
                  {#if process.threadId !== currentThreadId}
                    <span>·</span>
                    <span>other thread</span>
                  {/if}
                </span>
              </span>
            </button>
          {/each}
        </div>
      {/if}

      <div class="mt-2 border-t border-surface-200-800 pt-1.5">
        <button
          onclick={onShowSideQuestions}
          class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
        >
          <MessageCircleQuestion size={13} class="shrink-0 text-primary-500" />
          <span class="flex-1">Side questions</span>
          {#if sideQuestionCount > 0}
            <span class="rounded-full bg-primary-500/15 px-1.5 py-0.5 text-[10px] font-medium text-primary-500">{sideQuestionCount}</span>
          {:else}
            <Plus size={12} class="text-surface-500" />
          {/if}
        </button>
        <button
          onclick={onShowFiles}
          class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
        >
          <FolderTree size={13} class="shrink-0 text-surface-500" />
          <span class="flex-1">Files</span>
        </button>
        {#if messageLog.enabled}
          <button
            onclick={onShowMessageLog}
            class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-xs hover:preset-tonal"
          >
            <ScrollText size={13} class="shrink-0 text-surface-500" />
            <span class="flex-1">Message log</span>
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>
