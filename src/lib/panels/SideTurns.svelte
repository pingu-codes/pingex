<script lang="ts">
  import { ChevronRight } from "@lucide/svelte";
  import { Collapsible } from "@skeletonlabs/skeleton-svelte";
  import TypingDots from "$lib/components/TypingDots.svelte";
  import { approvals, elicitations, userInputRequests } from "$lib/services/codexEvents.svelte";
  import ApprovalCard from "$lib/thread/ApprovalCard.svelte";
  import ElicitationCard from "$lib/thread/ElicitationCard.svelte";
  import QuestionCard from "$lib/thread/QuestionCard.svelte";
  import ReasoningBlock from "$lib/thread/ReasoningBlock.svelte";
  import { upsertItem } from "$lib/thread/threadStream";
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
  import type { ThreadDetail, Turn } from "$lib/types";

  // A side question that runs as turns: its own turns, and the approvals,
  // questions and elicitations its thread raises.
  let {
    threadId,
    thread,
    turns,
    activeTurn,
  }: { threadId: string; thread: ThreadDetail; turns: Turn[]; activeTurn: Turn | null } = $props();

  const sideApprovals = $derived(approvals.list.filter((approval) => approval.threadId === threadId));
  const sideQuestionsPending = $derived(userInputRequests.list.filter((request) => request.threadId === threadId));
  const sideElicitations = $derived(elicitations.list.filter((entry) => entry.threadId === threadId));

  // Same rule as the main transcript: dots for every gap where Codex is thinking
  // rather than emitting; only text actively streaming in makes them redundant.
  const showTypingIndicator = $derived.by(() => {
    if (!activeTurn) return false;
    if (sideApprovals.length > 0 || sideQuestionsPending.length > 0 || sideElicitations.length > 0) return false;
    const last = activeTurn.items.at(-1);
    return !(last?.type === "agentMessage" && last.streaming);
  });
</script>

<div class="side-transcript space-y-3 text-xs">
  {#each turns as turn (turn.id)}
    {@const parts = splitTurn(turn, thread.harness)}
    {@const collapseDiffs = turnDiffCount(turn) > 1}
    {@const liveSegment = turn.status === "inProgress" ? parts.body.at(-1) : undefined}
    {@const firstWork = parts.body.find((segment) => segment.kind === "work")}
    {#each parts.users as item (item.id)}
      <div class="flex justify-end">
        <div
          class="max-w-[90%] rounded-xl rounded-br-sm bg-primary-500/10 px-3 py-2 text-xs leading-5 whitespace-pre-wrap"
        >
          {messageParts(item)
            .map((part) => part.text ?? "")
            .join("")}
        </div>
      </div>
    {/each}
    {#each parts.body as segment (completedSegmentKey(segment))}
      {#if segment.kind === "message"}
        <WorkItem item={segment.item} model={turn.model} effort={turn.reasoningEffort} />
      {:else if segment === liveSegment && !segment.collapsed}
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
            <Collapsible.Trigger
              class="group flex items-center gap-1 text-xs text-surface-500 hover:text-surface-700-300"
            >
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
          <hr class="mt-2 border-surface-200-800">
        </div>
      {/if}
    {/each}
    {#if turn.status === "failed" && turn.error}
      <div class="error-note p-3 space-y-2">
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
    <QuestionCard {request} onAnswered={(item) => upsertItem(thread.turns, request.turnId, item)} />
  {/each}
  {#each sideElicitations as elicitation (elicitation.requestId)}
    <ElicitationCard {elicitation} />
  {/each}

  {#if showTypingIndicator}
    <TypingDots />
  {/if}
</div>

<style>
  /* WorkItem sizes itself for the main transcript; the panel is narrow. */
  .side-transcript :global(.text-sm) {
    font-size: 0.75rem;
    line-height: 1.5;
  }
</style>
