<script lang="ts">
  import {
    Bot,
    Check,
    ChevronDown,
    Clock,
    Copy,
    FilePen,
    Globe,
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
  import type { Snippet } from "svelte";
  import { openSubagent } from "$lib/app/actions.svelte";
  import { BROWSER_NAMESPACE, describeBrowserCall } from "$lib/browser/browserActions";
  import DiffBlock from "$lib/components/DiffBlock.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { modelLabel } from "$lib/composer/models.svelte";
  import { commands, identity } from "$lib/services/homeRouting";
  import { openSettings } from "$lib/services/settingsNav.svelte";
  import { statusTone } from "$lib/thread/agentStatus";
  import QuestionCard from "$lib/thread/QuestionCard.svelte";
  import ThreadImage from "$lib/thread/ThreadImage.svelte";
  import { functionCallOutputText, reasoningContent, toolOutputImages } from "$lib/thread/turnSegments";
  import type { SubagentDetail, ThreadItem } from "$lib/types";
  import { copyCode, copyPlainOrMarkdown } from "$lib/utils/copy";
  import { flashFlag } from "$lib/utils/flash.svelte";
  import { renderMarkdown } from "$lib/utils/markdown";

  let {
    item,
    collapseDiffs = false,
    stranded,
    model = null,
    effort = null,
    subagents = [],
  }: {
    item: ThreadItem;
    collapseDiffs?: boolean;
    /** The thread's known subagents, so activity rows can name and open them. */
    subagents?: SubagentDetail[];
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
  const copied = flashFlag();
  const copiedCommand = flashFlag();

  /** Copies the command as the harness ran it, so it can be re-run verbatim. */
  function copyCommand() {
    commands.copyText(item.command ?? "").catch(() => {});
    copiedCommand.trigger();
  }

  /** Copies the rendered message by default, or its markdown source on Shift+click. */
  function copyMessage(event: MouseEvent) {
    // biome-ignore lint/performance/useDomNodeTextContent: innerText keeps the rendered line breaks the user sees.
    copyPlainOrMarkdown(event, messageEl?.innerText ?? item.text ?? "", item.text ?? "");
    copied.trigger();
  }

  /**
   * Codex's own collab agents report their activity as a separate item type,
   * alongside the `collabAgentToolCall` that carries the detail.
   */
  const SUB_AGENT_VERBS: Record<string, string> = {
    started: "Spawned agent",
    interacted: "Messaged agent",
    interrupted: "Stopped agent",
    completed: "Agent finished",
  };
  /**
   * The verb for a Codex collab tool call, keyed on the camelCase `tool` the
   * app-server sends (`CollabAgentTool` in the v2 protocol). The V1 tools and
   * the V2 ones are both listed: a thread can hold either generation.
   */
  const COLLAB_TOOL_VERBS: Record<string, string> = {
    spawnAgent: "Spawned",
    sendInput: "Sent input to",
    sendMessage: "Messaged",
    followupTask: "Sent a follow-up to",
    resumeAgent: "Resumed",
    wait: "Waited for",
    closeAgent: "Closed",
    interruptAgent: "Stopped",
    listAgents: "Listed",
  };
  /** "Spawned 3 subagents", "Waited for a subagent", "Listed agents". */
  function collabHeader(item: ThreadItem): string {
    const verb = COLLAB_TOOL_VERBS[item.tool ?? ""] ?? "Agent call";
    const count = collabAgentCount(item);
    if (item.tool === "listAgents") return `${verb} agents`;
    if (count > 1) return `${verb} ${count} subagents`;
    if (count === 1) return `${verb} a subagent`;
    return verb;
  }

  /**
   * A thread id as Codex wrote it. In a multi-home profile the listing's ids are
   * scoped while ids inside a re-read transcript are not, so lookups compare the
   * native half.
   */
  const nativeId = (id: string) => identity(id)?.nativeId ?? id;

  /**
   * The Codex subagent a row refers to, when the thread's listing has it. A row
   * can arrive before the listing refreshes, so the lookup may miss; the raw
   * agent path or id still renders then.
   */
  const subagentById = $derived(new Map(subagents.map((agent) => [nativeId(agent.id), agent])));
  const subagentFor = (id: string | undefined) => (id ? subagentById.get(nativeId(id)) : undefined);
  const activityAgent = (item: ThreadItem) => subagentFor(item.agentThreadId);
  const activityLabel = (item: ThreadItem, agent: SubagentDetail | undefined) =>
    agent?.agentNickname ?? agent?.agentRole ?? item.agentPath ?? "";
  /** Once an agent has finished or been stopped, its live status is stale news. */
  const activityShowsStatus = (item: ThreadItem) => item.kind === "started" || item.kind === "interacted";

  /**
   * The agents a collab call addressed, each with the state the call last saw.
   * `agentsStates` is keyed by native thread id whichever path the item came by.
   */
  function collabAgents(item: ThreadItem) {
    const states = item.agentsStates ?? {};
    const ids = item.receiverThreadIds?.length ? item.receiverThreadIds : Object.keys(states);
    return ids.map((id) => {
      const agent = subagentFor(id);
      return {
        id,
        agent,
        label: agent?.agentNickname ?? agent?.agentRole ?? nativeId(id).slice(0, 8),
        state: states[nativeId(id)],
      };
    });
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

  const commandStatusClass = (item: ThreadItem) =>
    item.exitCode === 0 || item.status === "completed"
      ? "bg-success-500"
      : item.status === "inProgress"
        ? "bg-warning-500"
        : "bg-error-500";
</script>

<!-- A one-line summary that unfolds into its detail, as tool calls, hook text
     and agent calls all render. -->
{#snippet expander(header: Snippet, body: Snippet, bodyClass = "")}
  <Collapsible class="min-w-0 items-stretch">
    <Collapsible.Trigger
      class="group flex w-full items-center gap-2 text-left text-xs text-surface-500 hover:text-surface-700-300"
    >
      {@render header()}
      <ChevronDown size={12} class="shrink-0 transition group-data-[state=open]:rotate-180" />
    </Collapsible.Trigger>
    <Collapsible.Content>
      <div class="mt-1.5 border-l-2 border-surface-200-800 pl-3 text-xs leading-5 {bodyClass}">{@render body()}</div>
    </Collapsible.Content>
  </Collapsible>
{/snippet}

<!-- What a running tool call is doing right now. -->
{#snippet liveLine(text: string)}
  <p class="mt-1 flex items-center gap-2 pl-5 text-[11px] leading-4 text-surface-500">
    <span class="shrink-0 animate-pulse text-primary-500" aria-hidden="true">▸</span>
    <span class="min-w-0 flex-1 truncate">{text}</span>
  </p>
{/snippet}

<!-- Images a tool returned (screenshots), drawn from their data: URLs. -->
{#snippet outputImages(urls: string[])}
  {#if urls.length > 0}
    <div class="flex flex-wrap gap-2">
      {#each urls as url, index (index)}
        <img
          src={url}
          alt="Tool result {index + 1}"
          class="max-h-64 max-w-full rounded-md border border-surface-200-800"
        >
      {/each}
    </div>
  {/if}
{/snippet}

{#snippet openThreadButton(open: () => void)}
  <button type="button" onclick={open} class="shrink-0 text-[10px] text-surface-500 hover:text-primary-500">
    Open thread
  </button>
{/snippet}

{#if item.type === "agentMessage" || item.type === "plan"}
  <div
    class="group/message min-w-0 {item.type === "agentMessage" && item.delivery === "async"
      ? "rounded-lg border border-transparent p-3 transition-colors hover:border-surface-300-700 focus-within:border-surface-300-700"
      : ""}"
  >
    {#if item.type === "agentMessage" && item.delivery === "async"}
      <span class="badge preset-tonal-surface mb-2 text-[10px]">Async</span>
    {/if}
    <div class="prose-md text-sm leading-7" bind:this={messageEl} use:copyCode>
      {@html renderMarkdown(item.text ?? "")}
    </div>
    <div
      class="mt-1 flex items-center gap-0.5 text-surface-500 transition-opacity {copied.on
        ? "opacity-100"
        : "opacity-0 group-hover/message:opacity-100"}"
    >
      <TooltipButton
        label="Copy message (⇧ for Markdown)"
        type="button"
        aria-label="Copy message"
        onclick={copyMessage}
        class="grid size-6 place-items-center rounded-md hover:bg-surface-200-800 hover:text-surface-800-200"
      >
        {#if copied.on}
          <Check size={13} class="text-success-500" />
        {:else}
          <Copy size={13} />
        {/if}
      </TooltipButton>
      {#if model || effort}
        <span class="ml-1 truncate font-mono text-[10px]" title="Generated by this model">
          {model ? modelLabel(model) : "Default model"}{effort ? ` · ${effort}` : ""}
        </span>
      {/if}
    </div>
  </div>
{:else if item.type === "contextCompaction"}
  <div class="flex items-center gap-2 text-[11px] text-surface-500" aria-label="Context compacted">
    <hr class="flex-1 border-surface-200-800">
    <Layers size={12} class="shrink-0" />
    <span>Context compacted</span>
    <hr class="flex-1 border-surface-200-800">
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
  <div class="prose-md prose-md-compact text-xs leading-5 text-surface-500" use:copyCode>
    {@html renderMarkdown((item.summary ?? []).filter(Boolean).join("\n\n"))}
  </div>
  {#if rawReasoning(item)}
    <!-- The unsummarised version, only when Codex sent one. Collapsed: it is
         several times longer than the summary above and repeats it. -->
    <Collapsible class="min-w-0 items-stretch">
      <Collapsible.Trigger
        class="group mt-1 flex items-center gap-1.5 text-[10px] text-surface-500 hover:text-surface-700-300"
      >
        <ChevronDown size={11} class="transition group-data-[state=open]:rotate-180" />
        Show full reasoning
      </Collapsible.Trigger>
      <Collapsible.Content>
        <div
          class="prose-md prose-md-compact mt-1 border-l-2 border-surface-200-800 pl-3 text-xs leading-5 text-surface-500"
          use:copyCode
        >
          {@html renderMarkdown(rawReasoning(item))}
        </div>
      </Collapsible.Content>
    </Collapsible>
  {/if}
{:else if item.type === "commandExecution"}
  <Collapsible class="min-w-0 items-stretch">
    <div class="group/command overflow-hidden panel-card">
      <div class="flex items-center pr-2">
        <Collapsible.Trigger class="group flex min-w-0 flex-1 items-center gap-2.5 px-3 py-2 text-left">
          <span class="size-1.5 shrink-0 rounded-full {commandStatusClass(item)}"></span>
          <Terminal size={13} class="shrink-0 text-surface-500" />
          <code class="min-w-0 flex-1 truncate font-mono text-xs text-surface-700-300" title={item.command}
            >{item.command}</code
          >
          {#if item.durationMs != null}
            <span class="shrink-0 text-[10px] text-surface-500">{(item.durationMs / 1000).toFixed(1)}s</span>
          {/if}
          <ChevronDown size={13} class="shrink-0 text-surface-500 transition group-data-[state=open]:rotate-180" />
        </Collapsible.Trigger>
        <TooltipButton
          label="Copy command"
          type="button"
          onclick={copyCommand}
          class="grid size-6 shrink-0 place-items-center rounded-md text-surface-500 transition-opacity hover:bg-surface-200-800 hover:text-surface-800-200 {copiedCommand.on
            ? "opacity-100"
            : "opacity-0 group-hover/command:opacity-100 focus-visible:opacity-100"}"
        >
          {#if copiedCommand.on}
            <Check size={13} class="text-success-500" />
          {:else}
            <Copy size={13} />
          {/if}
        </TooltipButton>
      </div>
      <Collapsible.Content>
        <!-- The header truncates; here the command wraps in full so long
             invocations can be read and selected. -->
        <div class="border-t border-surface-200-800 px-3 py-2">
          {#if item.cwd}
            <p class="mb-1 truncate font-mono text-[10px] text-surface-500" title={item.cwd}>{item.cwd}</p>
          {/if}
          <pre
            class="font-mono text-[11px] leading-5 break-all whitespace-pre-wrap text-surface-700-300"
          >{item.command}</pre>
        </div>
        <pre
          class="max-h-72 overflow-auto border-t border-surface-200-800 bg-surface-50-950 px-3 py-2.5 font-mono text-[11px] leading-5 break-all whitespace-pre-wrap text-surface-600-400"
        >{item.aggregatedOutput?.trim() || "No output"}</pre>
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
  {#snippet collabSummary()}
    <Bot size={12} class="shrink-0" />
    <span class="shrink-0">{collabHeader(item)}</span>
    <span class="min-w-0 flex-1 truncate font-mono text-[10px]">
      {#if item.model || item.reasoningEffort}
        {item.model ?? "default model"}
        · {item.reasoningEffort ?? "default effort"}
      {/if}
    </span>
  {/snippet}
  {#snippet collabDetail()}
    {#each collabAgents(item) as entry (entry.id)}
      <div class="flex items-center gap-2">
        <span class={entry.agent ? "text-surface-700-300" : "font-mono text-[10px] text-surface-500"}
          >{entry.label}</span
        >
        {#if entry.state}
          <span class={statusTone(entry.state.status)}>{entry.state.status}</span>
        {/if}
        <span class="min-w-0 flex-1 truncate text-surface-500">{entry.state?.message ?? ""}</span>
        {#if entry.agent}
          {@const agent = entry.agent}
          {@render openThreadButton(() => openSubagent(agent))}
        {/if}
      </div>
    {/each}
    {#if item.prompt}
      <div class="whitespace-pre-wrap break-words text-surface-500">{item.prompt}</div>
    {/if}
  {/snippet}
  {@render expander(collabSummary, collabDetail, "space-y-1.5")}
{:else if item.type === "subAgentActivity"}
  <!-- On Codex's path-based (V2) agents this row is all the parent transcript
       gets: the message sent lives only in the child thread, never on the
       parent's wire, so "Open thread" is the way to read it. -->
  {@const agent = activityAgent(item)}
  {@const label = activityLabel(item, agent)}
  <div class="flex items-center gap-2 text-xs">
    <Bot size={12} class="shrink-0 text-surface-500" />
    <span class="min-w-0 flex-1 truncate text-surface-500">
      {SUB_AGENT_VERBS[item.kind ?? ""] ?? "Agent"}
      {#if label}
        <span class="text-surface-700-300">{label}</span>
      {/if}
    </span>
    {#if item.kind === "started" && (item.model || item.reasoningEffort)}
      <span class="shrink-0 font-mono text-[10px] text-surface-500">
        {item.model ?? "default model"}
        · {item.reasoningEffort ?? "default effort"}
      </span>
    {/if}
    {#if agent}
      {#if activityShowsStatus(item) && agent.status}
        <span class={`shrink-0 ${statusTone(agent.status)}`}>{agent.status}</span>
      {/if}
      {@render openThreadButton(() => openSubagent(agent))}
    {/if}
  </div>
{:else if item.type === "enteredReviewMode" || item.type === "exitedReviewMode"}
  <div
    class="flex items-center gap-2 text-[11px] text-surface-500"
    aria-label={item.type === "enteredReviewMode" ? "Entered review mode" : "Left review mode"}
  >
    <hr class="flex-1 border-surface-200-800">
    <ScanEye size={12} class="shrink-0" />
    <span
      >{item.type === "enteredReviewMode" ? "Reviewing" : "Finished reviewing"}{item.review
        ? `: ${item.review}`
        : ""}</span
    >
    <hr class="flex-1 border-surface-200-800">
  </div>
{:else if item.type === "imageView"}
  {#if item.path}
    <ThreadImage path={item.path} label={item.path} />
  {:else}
    <div class="flex items-center gap-2 text-xs text-surface-500">
      <Image size={12} class="shrink-0" />
      <span class="min-w-0 flex-1 truncate font-mono text-[11px]">an image</span>
    </div>
  {/if}
{:else if item.type === "imageGeneration"}
  {#if item.savedPath}
    <ThreadImage path={item.savedPath} label={`Generated an image · ${item.savedPath}`} />
  {:else}
    <div class="flex items-center gap-2 text-xs text-surface-500">
      <Image size={12} class="shrink-0" />
      <span class="min-w-0 flex-1 truncate">Generated an image</span>
    </div>
  {/if}
{:else if item.type === "sleep"}
  <div class="flex items-center gap-2 text-xs text-surface-500">
    <Clock size={12} class="shrink-0" />
    <span>Waited {((item.durationMs ?? 0) / 1000).toFixed(1)}s</span>
  </div>
{:else if item.type === "hookPrompt"}
  <!-- Text one of the user's hooks pushed into the conversation. Codex reads it
       as if it came from the user, so it is shown rather than hidden — collapsed,
       because a hook can inject a lot of it. -->
  {#snippet hookSummary()}
    <Webhook size={12} class="shrink-0" />
    <span class="shrink-0">Hook added context</span>
    <span class="min-w-0 flex-1 truncate">{hookPromptText(item)}</span>
  {/snippet}
  {#snippet hookText()}
    {hookPromptText(item)}
  {/snippet}
  {@render expander(hookSummary, hookText, "whitespace-pre-wrap break-words text-surface-500")}
{:else if item.type === "functionCallOutput"}
  <!-- The raw result of a tool the model called (Codex ≥0.151). Text output
       is shown behind an expander; image parts are just counted. -->
  {#snippet outputSummary()}
    <Wrench size={12} class="shrink-0" />
    <span class="shrink-0 font-mono">{item.namespace ? `${item.namespace}/` : ""}{item.name ?? "tool"} output</span>
  {/snippet}
  {#snippet outputText()}
    {functionCallOutputText(item)}
    {@render outputImages(toolOutputImages(item))}
  {/snippet}
  {@render expander(outputSummary, outputText, "whitespace-pre-wrap break-words text-surface-500")}
{:else if item.type === "dynamicToolCall" && item.namespace === BROWSER_NAMESPACE}
  <!-- The agent acting in the user's browser: one line per action, the
       result (snapshot text, screenshot) behind the expander. -->
  {@const output = functionCallOutputText(item)}
  {@const images = toolOutputImages(item)}
  {@const failed = item.success === false || item.status === "failed"}
  {#snippet browserSummary()}
    <Globe size={12} class="shrink-0 {failed ? "text-error-500" : ""}" />
    <span class="min-w-0 flex-1 truncate">{describeBrowserCall(item.tool, item.arguments)}</span>
    {#if item.status === "inProgress"}
      <span class="shrink-0 text-[10px]">Running</span>
    {:else if failed}
      <span class="shrink-0 text-[10px] text-error-500">Failed</span>
    {/if}
  {/snippet}
  {#snippet browserDetail()}
    {#if output}
      <pre class="max-h-72 overflow-auto whitespace-pre-wrap break-words font-mono text-[11px] leading-4">{output}</pre>
    {/if}
    {@render outputImages(images)}
  {/snippet}
  {#if output || images.length > 0}
    {@render expander(browserSummary, browserDetail, "space-y-2 text-surface-500")}
  {:else}
    <div class="flex items-center gap-2 text-xs text-surface-500">{@render browserSummary()}</div>
  {/if}
{:else if item.type === "mcpToolCall" || item.type === "dynamicToolCall" || item.type === "webSearch"}
  {@const description = item.title || item.query}
  {@const preview =
    item.tool && description?.startsWith(`${item.tool} `) ? description.slice(item.tool.length + 1) : description}
  {@const output = functionCallOutputText(item)}
  {#if description || output}
    {#snippet toolSummary()}
      <Wrench size={12} class="shrink-0" />
      <span class="shrink-0 font-mono">{item.server ? `${item.server}/` : ""}{item.tool ?? item.type}</span>
      <span class="min-w-0 flex-1 truncate" title={description}>{preview}</span>
      {#if item.status}
        <span class="shrink-0 text-[10px]">{item.status === "inProgress" ? "Running" : item.status}</span>
      {/if}
    {/snippet}
    {#snippet toolDetail()}
      {#if description}
        <div>{description}</div>
      {/if}
      {#if output}
        <div>{output}</div>
      {/if}
      {@render outputImages(toolOutputImages(item))}
    {/snippet}
    {@render expander(toolSummary, toolDetail, "space-y-2 whitespace-pre-wrap break-words text-surface-500")}
  {:else}
    <div class="flex items-center gap-2 text-xs text-surface-500">
      <Wrench size={12} />
      <span class="font-mono">{item.server ? `${item.server}/` : ""}{item.tool ?? item.type}</span>
    </div>
  {/if}
  {#if item.progress}
    <!-- A long MCP call reporting in. Gone once the call completes, since the
         completed item carries no progress. -->
    {@render liveLine(item.progress)}
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
