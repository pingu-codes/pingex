<script lang="ts">
import {
  Bot,
  Check,
  ChevronDown,
  Clock,
  Copy,
  Ellipsis,
  FilePen,
  Image,
  Layers,
  MessageCircleQuestion,
  Plug,
  ScanEye,
  ShieldAlert,
  Terminal,
  Webhook,
  Wrench,
} from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { trackSubagent } from "$lib/app/appData.svelte";
// Agent threads are hidden from the sidebar on purpose, and `openThreadById`
// silently does nothing for a thread it cannot find there.
import { openThreadInCwd } from "$lib/app/navigation.svelte";
import DiffBlock from "$lib/components/DiffBlock.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { modelLabel } from "$lib/composer/models.svelte";
import { activityFor, agentClock, elapsedLabel, isRunning, runForToolCall } from "$lib/services/agentRuns.svelte";
import { copyText, killAgentRun } from "$lib/services/api";
import { openSettings } from "$lib/services/settingsNav.svelte";
import QuestionCard from "$lib/thread/QuestionCard.svelte";
import { reasoningContent } from "$lib/thread/turnSegments";
import type { AgentRun, ThreadItem } from "$lib/types";
import { copyCode } from "$lib/utils/copy";
import { renderMarkdown } from "$lib/utils/markdown";

let {
  item,
  collapseDiffs = false,
  stranded,
  model = null,
  effort = null,
}: {
  item: ThreadItem;
  collapseDiffs?: boolean;
  /** What the turn ran on, shown alongside the message's hover actions. */
  model?: string | null;
  effort?: string | null;
  /**
   * Supplied for a question the app never got to answer, so the transcript can
   * offer to answer it now as a new message rather than just record the gap.
   */
  stranded?: {
    threadId: string;
    turnId: string;
    onResume: (text: string) => Promise<void> | void;
    onAnswered: (item: ThreadItem) => void;
  };
} = $props();

let messageEl = $state<HTMLElement | null>(null);
let copied = $state(false);
let menuOpen = $state(false);
let copyTimer: ReturnType<typeof setTimeout> | undefined;

function flashCopied() {
  copied = true;
  clearTimeout(copyTimer);
  copyTimer = setTimeout(() => (copied = false), 1500);
}

/** Copies the message's markdown source, so formatting survives the paste. */
function copyMessageMarkdown() {
  copyText(item.text ?? "").catch(() => {});
  flashCopied();
}

/** Copies the rendered message with its markdown syntax stripped. */
function copyMessageText() {
  menuOpen = false;
  copyText(messageEl?.innerText ?? item.text ?? "").catch(() => {});
  flashCopied();
}

let menuWrap = $state<HTMLElement | null>(null);

function onWindowClick(event: MouseEvent) {
  if (menuOpen && menuWrap && !menuWrap.contains(event.target as Node)) menuOpen = false;
}

/**
 * The app's own agent tools, rendered as agent cards rather than as the
 * anonymous tool rows every other dynamic tool gets.
 */
const AGENT_TOOL_VERBS: Record<string, string> = {
  pingex_spawn_agent: "Spawned agent",
  pingex_wait_agents: "Waited for agents",
  pingex_send_input: "Sent input to",
  pingex_kill_agent: "Stopped agent",
};
/**
 * Codex's own collab agents report their activity as a separate item type,
 * alongside the `collabAgentToolCall` that carries the detail.
 */
const SUB_AGENT_VERBS: Record<string, string> = {
  started: "Spawned agent",
  interacted: "Messaged agent",
  interrupted: "Stopped agent",
};

const isAgentTool = (tool: string | undefined) => Boolean(tool && tool in AGENT_TOOL_VERBS);
const agentToolVerb = (tool: string | undefined) => AGENT_TOOL_VERBS[tool ?? ""] ?? "Agent";

/**
 * What to call the agent. The tool call carries the name the model chose, so
 * the row reads correctly even before the run's first event reaches the store
 * — and still reads correctly for a thread reopened long after the fact.
 */
function agentLabel(item: ThreadItem, runName: string | undefined): string {
  const argued = item.arguments?.name;
  if (typeof argued === "string" && argued.trim()) return argued.trim();
  if (runName) return runName;
  const ids = item.arguments?.agentIds;
  if (Array.isArray(ids) && ids.length) return `${ids.length} agent${ids.length === 1 ? "" : "s"}`;
  const id = item.arguments?.agentId;
  return typeof id === "string" ? id : "";
}

/**
 * What the parent sent this agent: the opening prompt on a spawn, the follow-up
 * on a `send_input`. Both are otherwise visible only inside the agent's own
 * thread, so from the parent's transcript the model appears to hand work off
 * without ever saying what the work is.
 */
function agentInputText(item: ThreadItem): string {
  const sent =
    item.tool === "pingex_spawn_agent"
      ? item.arguments?.prompt
      : item.tool === "pingex_send_input"
        ? item.arguments?.text
        : null;
  return typeof sent === "string" ? sent.trim() : "";
}

/**
 * Open the thread an agent ran in. Registered first: agent threads are kept out
 * of the sidebar, and without a record of one the app has no parent to send the
 * back arrow to and no way to resolve the thread at all.
 */
function openAgentRunThread(run: AgentRun) {
  if (!run.childThreadId) return;
  trackSubagent({
    id: run.childThreadId,
    parentThreadId: run.parentThreadId,
    title: run.name,
    cwd: run.cwd,
    status: run.status,
    agentNickname: run.name,
    agentRole: null,
    model: run.model,
    reasoningEffort: run.reasoningEffort,
    source: "app",
    runId: run.runId,
  });
  openThreadInCwd(run.childThreadId, run.cwd);
}

const rawReasoning = (item: ThreadItem) => reasoningContent(item).filter(Boolean).join("\n\n");

/**
 * Codex's verdict on an action it decided about without asking, worth showing
 * only when it went against the action or flagged real risk — an approved
 * low-risk command is just a command.
 */
function guardianVerdict(item: ThreadItem): string | null {
  const review = item.guardianReview;
  if (!review) return null;
  const notable = review.status !== "approved" || ["high", "critical"].includes(review.riskLevel ?? "");
  if (!notable) return null;
  const head = review.status === "denied" ? "Blocked by Codex" : `Codex review: ${review.status}`;
  const risk = review.riskLevel ? ` (${review.riskLevel} risk)` : "";
  return `${head}${risk}${review.rationale ? ` — ${review.rationale}` : ""}`;
}

/** A hook can contribute several fragments; they read as one block of text. */
const hookPromptText = (item: ThreadItem) =>
  (item.fragments ?? [])
    .map((fragment) => fragment.text)
    .filter(Boolean)
    .join("\n");

const collabAgentCount = (item: ThreadItem) => item.receiverThreadIds?.length ?? 0;
const collabStateClass = (status: string) =>
  ["errored", "failed", "systemError", "notFound"].includes(status)
    ? "text-error-500"
    : ["completed", "finished", "done", "shutdown"].includes(status)
      ? "text-surface-500"
      : "text-success-500";

const commandStatusClass = (item: ThreadItem) =>
  item.exitCode === 0 || item.status === "completed"
    ? "bg-success-500"
    : item.status === "inProgress"
      ? "bg-warning-500"
      : "bg-error-500";
</script>

<svelte:window onclick={onWindowClick} />

{#if item.type === "agentMessage" || item.type === "plan"}
  <div class="group/message min-w-0">
    <div class="prose-thread min-w-0 text-sm leading-7" bind:this={messageEl} use:copyCode>
      {@html renderMarkdown(item.text ?? "")}
    </div>
    <div
      class="mt-1 flex items-center gap-0.5 text-surface-500 transition-opacity {menuOpen || copied
        ? 'opacity-100'
        : 'opacity-0 group-hover/message:opacity-100'}"
    >
      <TooltipButton
        label="Copy message"
        type="button"
        aria-label="Copy message"
        onclick={copyMessageMarkdown}
        class="grid size-6 place-items-center rounded-md hover:bg-surface-200-800 hover:text-surface-800-200"
      >
        {#if copied}
          <Check size={13} class="text-success-500" />
        {:else}
          <Copy size={13} />
        {/if}
      </TooltipButton>
      <div class="relative" bind:this={menuWrap}>
        <button
          type="button"
          aria-label="Message options"
          onclick={() => (menuOpen = !menuOpen)}
          class="grid size-6 place-items-center rounded-md hover:bg-surface-200-800 hover:text-surface-800-200 {menuOpen ? 'bg-surface-200-800 text-surface-800-200' : ''}"
        >
          <Ellipsis size={13} />
        </button>
        {#if menuOpen}
          <div class="absolute top-full left-0 z-20 mt-1 w-44 rounded-lg border border-surface-200-800 bg-surface-50-950 p-1 shadow-lg">
            <button
              type="button"
              onclick={copyMessageText}
              class="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs text-surface-700-300 hover:bg-surface-200-800"
            >
              <Copy size={12} /> Copy as plain text
            </button>
          </div>
        {/if}
      </div>
      {#if model || effort}
        <span class="ml-1 truncate font-mono text-[10px]" title="Generated by this model">
          {model ? modelLabel(model) : "Default model"}{effort ? ` · ${effort}` : ""}
        </span>
      {/if}
    </div>
  </div>
{:else if item.type === "contextCompaction"}
  <div class="flex items-center gap-2 text-[11px] text-surface-500" aria-label="Context compacted">
    <hr class="flex-1 border-surface-200-800" />
    <Layers size={12} class="shrink-0" />
    <span>Context compacted</span>
    <hr class="flex-1 border-surface-200-800" />
  </div>
{:else if item.type === "userInputAnswered" && item.unanswered && stranded}
  <QuestionCard
    request={{
      requestId: null,
      threadId: stranded.threadId,
      turnId: stranded.turnId,
      itemId: item.id,
      questions: (item.questions ?? []).map((question) => ({ ...question, header: question.header ?? "" })),
    }}
    onResume={stranded.onResume}
    onAnswered={stranded.onAnswered}
  />
{:else if item.type === "userInputAnswered"}
  <div class="card preset-tonal space-y-2 p-3 text-sm">
    <div class="flex items-center gap-2 text-xs font-semibold">
      <MessageCircleQuestion size={14} class="text-primary-500" />
      Codex asked a question
      {#if item.dismissed}
        <span class="font-normal text-surface-500">· left unanswered</span>
      {/if}
    </div>
    {#each item.questions ?? [] as question (question.id)}
      <div class="space-y-1">
        {#if (item.questions?.length ?? 0) > 1 || question.header}
          <div class="text-[10px] font-semibold uppercase tracking-wide text-surface-500">{question.header}</div>
        {/if}
        <p class="text-xs leading-5 text-surface-600-400">{question.question}</p>
        {#if !item.steer && !item.dismissed}
          <p class="text-xs leading-5">
            <span class="font-medium">{(item.answers?.[question.id]?.answers ?? []).join(" · ") || "—"}</span>
          </p>
        {/if}
      </div>
    {/each}
    {#if item.steer}
      <div class="space-y-1 border-t border-surface-200-800 pt-2">
        <div class="text-[10px] font-semibold uppercase tracking-wide text-surface-500">Steered instead</div>
        <p class="text-xs font-medium leading-5">{item.steer}</p>
      </div>
    {/if}
  </div>
{:else if item.type === "reasoning"}
  <div class="prose-reasoning text-xs leading-5 text-surface-500" use:copyCode>
    {@html renderMarkdown((item.summary ?? []).filter(Boolean).join("\n\n"))}
  </div>
  {#if rawReasoning(item)}
    <!-- The unsummarised version, only when Codex sent one. Collapsed: it is
         several times longer than the summary above and repeats it. -->
    <Collapsible>
      <Collapsible.Trigger class="group mt-1 flex items-center gap-1.5 text-[10px] text-surface-500 hover:text-surface-700-300">
        <ChevronDown size={11} class="transition group-data-[state=open]:rotate-180" />
        Show full reasoning
      </Collapsible.Trigger>
      <Collapsible.Content>
        <div class="prose-reasoning mt-1 border-l-2 border-surface-200-800 pl-3 text-xs leading-5 text-surface-500" use:copyCode>
          {@html renderMarkdown(rawReasoning(item))}
        </div>
      </Collapsible.Content>
    </Collapsible>
  {/if}
{:else if item.type === "commandExecution"}
  <Collapsible>
    <div class="overflow-hidden rounded-xl border border-surface-200-800 bg-surface-100-900">
      <Collapsible.Trigger class="group flex w-full items-center gap-2.5 px-3 py-2 text-left">
        <span class="size-1.5 shrink-0 rounded-full {commandStatusClass(item)}"></span>
        <Terminal size={13} class="shrink-0 text-surface-500" />
        <code class="min-w-0 flex-1 truncate font-mono text-xs text-surface-700-300">{item.command}</code>
        {#if item.durationMs != null}
          <span class="shrink-0 text-[10px] text-surface-500">{(item.durationMs / 1000).toFixed(1)}s</span>
        {/if}
        <ChevronDown size={13} class="shrink-0 text-surface-500 transition group-data-[state=open]:rotate-180" />
      </Collapsible.Trigger>
      <Collapsible.Content>
        <pre class="max-h-72 overflow-auto border-t border-surface-200-800 bg-surface-50-950 px-3 py-2.5 font-mono text-[11px] leading-5 text-surface-600-400">{item.aggregatedOutput?.trim() || "No output"}</pre>
      </Collapsible.Content>
    </div>
  </Collapsible>
  {#if guardianVerdict(item)}
    <!-- Codex judged this itself and never asked. Say why, or a blocked command
         reads as an unexplained failure. -->
    <p class="mt-1 flex items-start gap-1.5 pl-1 text-[11px] leading-4 text-warning-500">
      <ShieldAlert size={11} class="mt-px shrink-0" />
      <span class="min-w-0">{guardianVerdict(item)}</span>
    </p>
  {/if}
{:else if item.type === "fileChange"}
  {#if (item.changes?.length ?? 0) === 0}
    <!-- The edit has started but the patch has not landed yet; without this the
         item renders as nothing and the thread looks stalled. -->
    <div class="flex items-center gap-2 text-xs text-surface-500" aria-label="Editing files">
      <FilePen size={12} class="shrink-0 animate-pulse" />
      <span>Editing files…</span>
    </div>
  {:else}
    <div class="space-y-3">
      {#each item.changes ?? [] as change (change.path)}
        <DiffBlock {change} autoCollapse={collapseDiffs || (item.changes?.length ?? 0) > 1} />
      {/each}
    </div>
  {/if}
{:else if item.type === "collabAgentToolCall"}
  <Collapsible>
    <Collapsible.Trigger class="group flex w-full items-center gap-2 text-left text-xs text-surface-500 hover:text-surface-700-300">
      <Bot size={12} class="shrink-0" />
      <span class="shrink-0">
        {collabAgentCount(item) > 1 ? `Spawned ${collabAgentCount(item)} subagents` : "Spawned a subagent"}
      </span>
      <span class="min-w-0 flex-1 truncate font-mono text-[10px]">
        {item.model ?? "default model"} · {item.reasoningEffort ?? "default effort"}
      </span>
      <ChevronDown size={12} class="shrink-0 transition group-data-[state=open]:rotate-180" />
    </Collapsible.Trigger>
    <Collapsible.Content>
      <div class="mt-1.5 space-y-1.5 border-l-2 border-surface-200-800 pl-3 text-xs leading-5">
        {#each Object.entries(item.agentsStates ?? {}) as [agentId, agentState] (agentId)}
          <div class="flex items-center gap-2">
            <span class="font-mono text-[10px] text-surface-500">{agentId.slice(0, 8)}</span>
            <span class={collabStateClass(agentState.status)}>{agentState.status}</span>
            {#if agentState.message}<span class="min-w-0 flex-1 truncate text-surface-500">{agentState.message}</span>{/if}
          </div>
        {/each}
        {#if item.prompt}
          <div class="whitespace-pre-wrap break-words text-surface-500">{item.prompt}</div>
        {/if}
      </div>
    </Collapsible.Content>
  </Collapsible>
{:else if item.type === "dynamicToolCall" && isAgentTool(item.tool)}
  {@const run = runForToolCall(item)}
  {@const label = agentLabel(item, run?.name)}
  {@const activity = run ? activityFor(run) : null}
  {@const sent = agentInputText(item)}
  <div class="flex items-center gap-2 text-xs">
    <Bot size={12} class="shrink-0 text-surface-500" />
    <span class="min-w-0 flex-1 truncate text-surface-500">
      {agentToolVerb(item.tool)}
      {#if label}<span class="text-surface-700-300">{label}</span>{/if}
    </span>
    {#if run}
      <span class={`shrink-0 ${collabStateClass(run.status)}`}>{run.status}</span>
      {#if run.childThreadId}
        <button
          type="button"
          onclick={() => run.childThreadId && openAgentRunThread(run)}
          class="shrink-0 text-[10px] text-surface-500 hover:text-primary-500"
        >
          Open thread
        </button>
      {/if}
      {#if isRunning(run)}
        <button
          type="button"
          onclick={() => killAgentRun(run.runId)}
          class="shrink-0 text-[10px] text-surface-500 hover:text-error-500"
        >
          Stop
        </button>
      {/if}
    {/if}
  </div>
  {#if sent}
    <p class="mt-1 line-clamp-3 whitespace-pre-wrap break-words pl-5 text-[11px] leading-4 text-surface-500">
      {sent}
    </p>
  {/if}
  {#if activity}
    <div class="mt-1 flex items-center gap-2 pl-5 text-[11px] leading-4 text-surface-500">
      <span class="shrink-0 animate-pulse text-primary-500" aria-hidden="true">▸</span>
      <span class="min-w-0 flex-1 truncate">{activity.label}</span>
      <span class="shrink-0 tabular-nums">{elapsedLabel(activity.since, agentClock.now)}</span>
    </div>
  {/if}
  {#if run && !isRunning(run) && run.result}
    <p class="mt-1 whitespace-pre-wrap break-words pl-5 text-[11px] leading-4 text-surface-600-400">
      {run.result}
    </p>
  {/if}
  {#if run?.error}
    <p class="mt-1 pl-5 text-[11px] leading-4 text-error-500">{run.error}</p>
  {/if}
{:else if item.type === "subAgentActivity"}
  <div class="flex items-center gap-2 text-xs">
    <Bot size={12} class="shrink-0 text-surface-500" />
    <span class="min-w-0 flex-1 truncate text-surface-500">
      {SUB_AGENT_VERBS[item.kind ?? ""] ?? "Agent"}
      {#if item.agentPath}<span class="text-surface-700-300">{item.agentPath}</span>{/if}
    </span>
  </div>
{:else if item.type === "enteredReviewMode" || item.type === "exitedReviewMode"}
  <div
    class="flex items-center gap-2 text-[11px] text-surface-500"
    aria-label={item.type === "enteredReviewMode" ? "Entered review mode" : "Left review mode"}
  >
    <hr class="flex-1 border-surface-200-800" />
    <ScanEye size={12} class="shrink-0" />
    <span>{item.type === "enteredReviewMode" ? "Reviewing" : "Finished reviewing"}{item.review ? `: ${item.review}` : ""}</span>
    <hr class="flex-1 border-surface-200-800" />
  </div>
{:else if item.type === "imageView"}
  <div class="flex items-center gap-2 text-xs text-surface-500">
    <Image size={12} class="shrink-0" />
    <span class="min-w-0 flex-1 truncate font-mono text-[11px]">{item.path ?? "an image"}</span>
  </div>
{:else if item.type === "imageGeneration"}
  <div class="flex items-center gap-2 text-xs text-surface-500">
    <Image size={12} class="shrink-0" />
    <span class="min-w-0 flex-1 truncate">
      Generated an image{item.savedPath ? ` · ${item.savedPath}` : ""}
    </span>
  </div>
{:else if item.type === "sleep"}
  <div class="flex items-center gap-2 text-xs text-surface-500">
    <Clock size={12} class="shrink-0" />
    <span>Waited {((item.durationMs ?? 0) / 1000).toFixed(1)}s</span>
  </div>
{:else if item.type === "hookPrompt"}
  <!-- Text one of the user's hooks pushed into the conversation. Codex reads it
       as if it came from the user, so it is shown rather than hidden — collapsed,
       because a hook can inject a lot of it. -->
  <Collapsible>
    <Collapsible.Trigger class="group flex w-full items-center gap-2 text-left text-xs text-surface-500 hover:text-surface-700-300">
      <Webhook size={12} class="shrink-0" />
      <span class="shrink-0">Hook added context</span>
      <span class="min-w-0 flex-1 truncate">{hookPromptText(item)}</span>
      <ChevronDown size={12} class="shrink-0 transition group-data-[state=open]:rotate-180" />
    </Collapsible.Trigger>
    <Collapsible.Content>
      <div class="mt-1.5 whitespace-pre-wrap break-words border-l-2 border-surface-200-800 pl-3 text-xs leading-5 text-surface-500">{hookPromptText(item)}</div>
    </Collapsible.Content>
  </Collapsible>
{:else if item.type === "mcpToolCall" || item.type === "dynamicToolCall" || item.type === "webSearch"}
  {#if item.query}
    <Collapsible>
      <Collapsible.Trigger class="group flex w-full items-center gap-2 text-left text-xs text-surface-500 hover:text-surface-700-300">
        <Wrench size={12} class="shrink-0" />
        <span class="shrink-0 font-mono">{item.server ? `${item.server}/` : ""}{item.tool ?? item.type}</span>
        <span class="min-w-0 flex-1 truncate">— {item.query}</span>
        <ChevronDown size={12} class="shrink-0 transition group-data-[state=open]:rotate-180" />
      </Collapsible.Trigger>
      <Collapsible.Content>
        <div class="mt-1.5 border-l-2 border-surface-200-800 pl-3 text-xs leading-5 text-surface-500 whitespace-pre-wrap break-words">{item.query}</div>
      </Collapsible.Content>
    </Collapsible>
  {:else}
    <div class="flex items-center gap-2 text-xs text-surface-500">
      <Wrench size={12} />
      <span class="font-mono">{item.server ? `${item.server}/` : ""}{item.tool ?? item.type}</span>
    </div>
  {/if}
  {#if item.progress}
    <!-- A long MCP call reporting in. Gone once the call completes, since the
         completed item carries no progress. -->
    <p class="mt-1 flex items-center gap-2 pl-5 text-[11px] leading-4 text-surface-500">
      <span class="shrink-0 animate-pulse text-primary-500" aria-hidden="true">▸</span>
      <span class="min-w-0 flex-1 truncate">{item.progress}</span>
    </p>
  {/if}
  {#if item.type === "mcpToolCall" && item.server}
    <button
      type="button"
      onclick={() => openSettings("integrations", item.server, item.tool ?? null)}
      class="mt-1 inline-flex items-center gap-1 text-[10px] text-surface-500 hover:text-primary-500"
    >
      <Plug size={10} /> {item.tool ? "View this tool" : "View integration"}
    </button>
  {/if}
{/if}

<style>
  .prose-reasoning :global(p) {
    margin: 0.25rem 0;
  }
  .prose-thread :global(p) {
    margin: 0.5rem 0;
  }
  .prose-thread :global(ul),
  .prose-thread :global(ol) {
    margin: 0.5rem 0;
    padding-left: 1.25rem;
  }
  .prose-thread :global(ul) {
    list-style: disc;
  }
  .prose-thread :global(ol) {
    list-style: decimal;
  }
  .prose-thread :global(h1),
  .prose-thread :global(h2),
  .prose-thread :global(h3) {
    margin: 1rem 0 0.5rem;
    font-weight: 600;
  }
  .prose-thread {
    overflow-wrap: anywhere;
    min-width: 0;
  }
  .prose-thread :global(a) {
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .prose-thread :global(code:not(.hljs)) {
    border-radius: 0.3rem;
    background: color-mix(in oklab, currentColor 10%, transparent);
    padding: 0.1rem 0.35rem;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 0.85em;
  }
  .prose-thread :global(blockquote) {
    margin: 0.5rem 0;
    border-left: 3px solid color-mix(in oklab, currentColor 25%, transparent);
    padding-left: 0.75rem;
    opacity: 0.85;
  }
  .prose-thread :global(.table-wrap) {
    margin: 0.75rem 0;
    overflow-x: auto;
  }
  .prose-thread :global(table) {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9em;
    line-height: 1.5;
  }
  .prose-thread :global(th),
  .prose-thread :global(td) {
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    padding: 0.35rem 0.75rem;
    text-align: left;
    vertical-align: top;
  }
  .prose-thread :global(th) {
    background: color-mix(in oklab, currentColor 6%, transparent);
    font-weight: 600;
  }
  .prose-thread :global(.code-block) {
    margin: 0.75rem 0;
    overflow: hidden;
    border-radius: 0.75rem;
    background: #0d1117;
  }
  .prose-thread :global(.code-lang) {
    padding: 0.4rem 0.9rem 0;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #7d8590;
  }
  .prose-thread :global(pre) {
    overflow-x: auto;
    padding: 0.65rem 0.9rem;
    font-family: var(--font-mono, ui-monospace, monospace);
    font-size: 12px;
    line-height: 1.6;
  }
  .prose-thread :global(pre code.hljs) {
    display: block;
    background: transparent;
    padding: 0;
  }
</style>
