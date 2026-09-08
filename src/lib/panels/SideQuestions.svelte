<script lang="ts">
import { ArrowUp, ChevronRight, MessageCircleQuestion, Square, Trash2 } from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { untrack } from "svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import TypingDots from "$lib/components/TypingDots.svelte";
import {
  addSideQuestion,
  forkThread,
  interruptTurn,
  invalidateThreadCache,
  readThread,
  removeSideQuestion,
  startTurn,
} from "$lib/services/api";
import {
  activeTurns,
  approvals,
  type CodexEvent,
  elicitations,
  setThreadHandler,
  userInputRequests,
} from "$lib/services/codexEvents.svelte";
import { threadIdOf } from "$lib/services/turnLifecycle";
import ApprovalCard from "$lib/thread/ApprovalCard.svelte";
import ElicitationCard from "$lib/thread/ElicitationCard.svelte";
import QuestionCard from "$lib/thread/QuestionCard.svelte";
import ReasoningBlock from "$lib/thread/ReasoningBlock.svelte";
import { applyThreadEvent, finalizeRunningTurns, upsertItem } from "$lib/thread/threadStream";
import {
  completedSegmentKey,
  messageParts,
  segmentKey,
  splitTurn,
  turnDiffCount,
  turnSegments,
  workedLabel,
} from "$lib/thread/turnSegments";
import WorkItem from "$lib/thread/WorkItem.svelte";
import type { BootstrapData, SideQuestion, ThreadDetail, Turn } from "$lib/types";

let {
  parentThreadId,
  sideQuestions,
  activeSideId = $bindable(null),
  onDataChanged,
}: {
  parentThreadId: string | null;
  sideQuestions: SideQuestion[];
  activeSideId?: string | null;
  onDataChanged: (data: BootstrapData) => void;
} = $props();

// How long Stop waits for Codex to confirm before the panel ends the turn on
// its own. Codex answers "no active turn" (no event follows) for a turn that
// already died; without this the composer would stay locked on it.
const STOP_GRACE_MS = 1500;

let sideThread = $state<ThreadDetail | null>(null);
let sideLoading = $state(false);
let starting = $state(false);
let sideError = $state<string | null>(null);
let question = $state("");
let lastParentId: string | null | undefined;
// The side thread `ask()` populated itself: the load effect must not replace it
// with a read that races the turn it just started.
let askedId: string | null = null;
let pendingTurnStart: Promise<Turn> | null = null;
let stopTimer: ReturnType<typeof setTimeout> | undefined;

const mine = $derived(sideQuestions.filter((entry) => entry.parentThreadId === parentThreadId));
const active = $derived(mine.find((entry) => entry.sideThreadId === activeSideId) ?? null);
// The fork carries the parent's history so the model has full context, but the
// panel shows only the side conversation. Questions recorded before the fork
// point was tracked have no count and show the whole fork.
const visibleTurns = $derived(sideThread?.turns.slice(active?.inheritedTurns ?? 0) ?? []);
const activeTurn = $derived(sideThread?.turns.find((turn) => turn.status === "inProgress") ?? null);
const busy = $derived(activeTurn !== null || starting);

const sideApprovals = $derived(approvals.list.filter((approval) => approval.threadId === activeSideId));
const sideQuestionsPending = $derived(userInputRequests.list.filter((request) => request.threadId === activeSideId));
const sideElicitations = $derived(elicitations.list.filter((entry) => entry.threadId === activeSideId));

// Same rule as the main transcript: dots for every gap where Codex is thinking
// rather than emitting; only text actively streaming in makes them redundant.
const showTypingIndicator = $derived.by(() => {
  if (!activeTurn) return false;
  if (sideApprovals.length > 0 || sideQuestionsPending.length > 0 || sideElicitations.length > 0) return false;
  const last = activeTurn.items.at(-1);
  return !(last?.type === "agentMessage" && last.streaming);
});

// The panel can outlive a thread switch; a stale activeSideId would route the
// next question into the previous thread's side conversation.
$effect(() => {
  if (lastParentId === undefined || parentThreadId === lastParentId) {
    lastParentId = parentThreadId;
    return;
  }
  lastParentId = parentThreadId;
  clearTimeout(stopTimer);
  activeSideId = null;
  sideThread = null;
  starting = false;
  sideError = null;
  question = "";
});

$effect(() => {
  const id = activeSideId;
  if (!id) {
    sideThread = null;
    return;
  }
  if (id.startsWith("preview-")) return;
  if (id === askedId) return;
  sideLoading = true;
  sideError = null;
  untrack(() => void load(id));
});

async function load(id: string) {
  try {
    // The cached detail is keyed by the summary's `updated_at`, which does not
    // move while a turn runs — for a working thread it is stale by construction.
    if (activeTurns.list.includes(id)) await invalidateThreadCache(id).catch(() => {});
    const detail = await readThread(id);
    if (id !== activeSideId) return;
    // A turn left `inProgress` by a session that has since died would keep the
    // composer locked forever — nothing can complete it, so show it as it is.
    if (!activeTurns.list.includes(id)) finalizeRunningTurns(detail.turns, "interrupted");
    sideThread = detail;
  } catch (cause) {
    if (id === activeSideId) sideError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (id === activeSideId) sideLoading = false;
  }
}

$effect(() => setThreadHandler(handleEvent));

function handleEvent(event: CodexEvent) {
  const { method, params } = event;
  if (method === "disconnected") {
    for (const turn of sideThread?.turns ?? []) {
      if (turn.status === "inProgress") turn.status = "interrupted";
    }
    return;
  }
  if (!sideThread || !params || threadIdOf(event) !== activeSideId) return;
  // Codex dropped the thread from memory: nothing can still be running in it.
  if (method === "thread/closed") {
    finalizeRunningTurns(sideThread.turns, "interrupted");
    return;
  }
  const outcome = applyThreadEvent(sideThread, event);
  if (outcome.streamError) sideError = outcome.streamError;
  if (outcome.turnCompleted) {
    clearTimeout(stopTimer);
    if (activeSideId) invalidateThreadCache(activeSideId).catch(() => {});
  }
}

async function ask() {
  const text = question.trim();
  if (!text || !parentThreadId || busy) return;
  question = "";
  sideError = null;
  starting = true;
  const localTurnId = `local-${Date.now()}`;
  try {
    // Only reuse the open side question if it belongs to this thread — a
    // stale id from a previous thread must fork fresh.
    let id = activeSideId && mine.some((entry) => entry.sideThreadId === activeSideId) ? activeSideId : null;
    if (!id) {
      const forked = await forkThread(parentThreadId);
      id = forked.id;
      // Read the fork before recording it: the turns it carries at this point
      // are the parent's, and the panel hides exactly that many.
      const inherited = await readThread(id);
      askedId = id;
      activeSideId = id;
      sideThread = { ...inherited, turns: [...inherited.turns] };
      onDataChanged(await addSideQuestion(parentThreadId, id, text, inherited.turns.length));
    }
    sideThread?.turns.push({
      id: localTurnId,
      status: "inProgress",
      items: [{ type: "userMessage", id: `local-item-${Date.now()}`, content: [{ type: "text", text }] }],
    });
    const start = startTurn(id, [{ type: "text", text }]);
    pendingTurnStart = start;
    const turn = await start;
    const pending = sideThread?.turns.find((candidate) => candidate.id === localTurnId);
    if (pending) {
      pending.id = turn.id;
      pending.status = turn.status ?? "inProgress";
    }
  } catch (cause) {
    if (sideThread) sideThread.turns = sideThread.turns.filter((candidate) => candidate.id !== localTurnId);
    sideError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    pendingTurnStart = null;
    starting = false;
  }
}

async function stop() {
  const id = activeSideId;
  let target = activeTurn;
  if (!id || !target) return;
  if (target.id.startsWith("local-")) {
    // The optimistic turn is still waiting on `turn/start`; Codex has never
    // heard of it. Wait for the real id, then interrupt that.
    try {
      await pendingTurnStart;
    } catch {
      return; // ask() already surfaced the error and removed the turn.
    }
    target = activeTurn;
    if (id !== activeSideId || !target || target.id.startsWith("local-")) return;
  }
  try {
    await interruptTurn(id, target.id);
  } catch (cause) {
    sideError = cause instanceof Error ? cause.message : String(cause);
  }
  // Stop is final: if no `turn/completed` confirms it, end the turn locally so
  // the composer comes back.
  clearTimeout(stopTimer);
  stopTimer = setTimeout(() => {
    if (id !== activeSideId || !sideThread) return;
    finalizeRunningTurns(sideThread.turns, "interrupted");
    invalidateThreadCache(id).catch(() => {});
  }, STOP_GRACE_MS);
}

async function deleteSide(entry: SideQuestion) {
  try {
    onDataChanged(await removeSideQuestion(entry.sideThreadId));
    if (activeSideId === entry.sideThreadId) {
      clearTimeout(stopTimer);
      activeSideId = null;
      sideThread = null;
    }
  } catch (cause) {
    sideError = cause instanceof Error ? cause.message : String(cause);
  }
}

function openSide(entry: SideQuestion) {
  askedId = null;
  activeSideId = entry.sideThreadId;
  sideThread = null;
}
</script>

<div class="min-h-0 flex-1 overflow-y-auto p-3">
  {#if !activeSideId}
    {#if mine.length === 0}
      <p class="text-xs leading-5 text-surface-500">
        Ask a question about this thread without adding it to the main conversation. The side question runs on a fork
        with full context.
      </p>
    {:else}
      <div class="space-y-1">
        {#each mine as entry (entry.sideThreadId)}
          <div class="group/side relative">
            <button
              onclick={() => openSide(entry)}
              class="flex w-full items-center gap-2 rounded-lg px-2.5 py-2 pr-8 text-left text-xs hover:preset-tonal"
            >
              <MessageCircleQuestion size={13} class="shrink-0 text-primary-500" />
              <span class="min-w-0 flex-1 truncate">{entry.title}</span>
            </button>
            <TooltipButton
              label="Delete side question"
              aria-label={`Delete side question ${entry.title}`}
              onclick={() => deleteSide(entry)}
              class="absolute right-1.5 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-300-700 hover:text-error-500 group-hover/side:opacity-100"
            >
              <Trash2 size={12} />
            </TooltipButton>
          </div>
        {/each}
      </div>
    {/if}
  {:else if sideLoading}
    <div class="space-y-2" aria-label="Loading side question">
      <div class="placeholder h-12 animate-pulse rounded-lg"></div>
      <div class="placeholder h-20 animate-pulse rounded-lg opacity-70"></div>
    </div>
  {:else if sideThread}
    <div class="side-transcript space-y-3 text-xs">
      {#each visibleTurns as turn (turn.id)}
        {@const parts = splitTurn(turn)}
        {@const collapseDiffs = turnDiffCount(turn) > 1}
        {@const liveSegment = turn.status === "inProgress" ? parts.body.at(-1) : undefined}
        {@const firstWork = parts.body.find((segment) => segment.kind === "work")}
        {#each parts.users as item (item.id)}
          <div class="flex justify-end">
            <div class="max-w-[90%] rounded-xl rounded-br-sm bg-primary-500/10 px-3 py-2 text-xs leading-5 whitespace-pre-wrap">
              {messageParts(item).map((part) => part.text ?? "").join("")}
            </div>
          </div>
        {/each}
        {#each parts.body as segment (completedSegmentKey(segment))}
          {#if segment.kind === "message"}
            <WorkItem item={segment.item} model={turn.model} effort={turn.reasoningEffort} />
          {:else if segment === liveSegment}
            {@const liveSegments = turnSegments(segment.items)}
            {#each liveSegments as liveSeg, liveIndex (segmentKey(liveSeg))}
              {#if liveSeg.kind === "reasoning"}
                <ReasoningBlock items={liveSeg.items} live={liveIndex === liveSegments.length - 1} />
              {:else}
                <WorkItem item={liveSeg.item} {collapseDiffs} />
              {/if}
            {/each}
          {:else}
            <div>
              <Collapsible class="min-w-0 items-stretch">
                <Collapsible.Trigger class="group flex items-center gap-1 text-xs text-surface-500 hover:text-surface-700-300">
                  <span>{segment === firstWork && turn.status !== "inProgress" ? workedLabel(turn) : "Worked"}</span>
                  <ChevronRight size={12} class="transition group-data-[state=open]:rotate-90" />
                </Collapsible.Trigger>
                <Collapsible.Content>
                  <div class="mt-2 space-y-3 border-l-2 border-surface-200-800 pl-3">
                    {#each segment.items as item (item.id)}
                      <WorkItem {item} {collapseDiffs} />
                    {/each}
                  </div>
                </Collapsible.Content>
              </Collapsible>
              <hr class="mt-2 border-surface-200-800" />
            </div>
          {/if}
        {/each}
        {#if turn.status === "failed" && turn.error}
          <div class="card preset-tonal-error space-y-2 p-3 text-xs">
            <div>{turn.error.message}</div>
            {#if turn.error.misalignment?.detailedExplanation}
              <p class="whitespace-pre-wrap opacity-90">{turn.error.misalignment.detailedExplanation}</p>
            {/if}
          </div>
        {/if}
      {/each}

      {#each sideApprovals as approval (approval.requestId)}
        <ApprovalCard {approval} />
      {/each}
      {#each sideQuestionsPending as request (request.requestId)}
        <QuestionCard {request} onAnswered={(item) => sideThread && upsertItem(sideThread.turns, request.turnId, item)} />
      {/each}
      {#each sideElicitations as elicitation (elicitation.requestId)}
        <ElicitationCard {elicitation} />
      {/each}

      {#if showTypingIndicator}
        <TypingDots />
      {/if}
    </div>
  {/if}
  {#if sideError}
    <div class="card preset-tonal-error mt-3 px-3 py-2 text-xs">{sideError}</div>
  {/if}
</div>

<div class="border-t border-surface-200-800 p-2">
  <div class="flex items-end gap-2 rounded-xl border border-surface-200-800 bg-surface-50-950 px-2.5 py-1.5 focus-within:border-surface-400-600">
    <textarea
      bind:value={question}
      onkeydown={(event) => {
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          ask();
        } else if (event.key === "Escape" && busy) {
          event.preventDefault();
          stop();
        }
      }}
      rows="1"
      placeholder={activeSideId ? "Follow up…" : "Ask a side question…"}
      class="max-h-24 min-h-[1.25rem] flex-1 resize-none bg-transparent text-xs leading-5 outline-none placeholder:text-surface-500"
    ></textarea>
    {#if busy}
      <TooltipButton
        label="Stop (Esc)"
        onclick={stop}
        aria-label="Stop side question"
        class="grid size-6 shrink-0 place-items-center rounded-full preset-filled-error-500"
      >
        <Square size={10} fill="currentColor" />
      </TooltipButton>
    {/if}
    <TooltipButton
      label={busy ? "Wait for the answer or press Stop" : "Ask side question"}
      onclick={ask}
      aria-label="Ask side question"
      disabled={busy || !question.trim()}
      class="grid size-6 shrink-0 place-items-center rounded-full preset-filled-primary-500 disabled:opacity-40"
    >
      <ArrowUp size={12} />
    </TooltipButton>
  </div>
</div>

<style>
  /* WorkItem sizes itself for the main transcript; the panel is narrow. */
  .side-transcript :global(.text-sm) {
    font-size: 0.75rem;
    line-height: 1.5;
  }
</style>
