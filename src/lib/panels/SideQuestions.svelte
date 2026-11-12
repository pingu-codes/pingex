<script lang="ts">
  import { untrack } from "svelte";
  import SideComposer from "$lib/panels/SideComposer.svelte";
  import SideExchanges from "$lib/panels/SideExchanges.svelte";
  import SideQuestionList from "$lib/panels/SideQuestionList.svelte";
  import SideTurns from "$lib/panels/SideTurns.svelte";
  import { openSideConversation, type SideConversation } from "$lib/panels/sideConversation.svelte";
  import type { BootstrapData, SideQuestion } from "$lib/types";

  let {
    parentThreadId,
    sideQuestions,
    activeSideId = $bindable(null),
    onDataChanged,
    submission = null,
  }: {
    parentThreadId: string | null;
    sideQuestions: SideQuestion[];
    activeSideId?: string | null;
    onDataChanged: (data: BootstrapData) => void;
    submission?: { id: number; text: string } | null;
  } = $props();

  let conversation = $state.raw<SideConversation | null>(null);
  let failure = $state<string | null>(null);
  let question = $state("");
  let lastParentId: string | null | undefined;
  let lastSubmission: number | undefined;

  // One conversation per parent thread. The panel can outlive a thread switch;
  // a stale activeSideId would route the next question into the previous
  // thread's side conversation.
  $effect(() => {
    const parent = parentThreadId;
    untrack(() => {
      if (lastParentId !== undefined && parent !== lastParentId) {
        activeSideId = null;
        question = "";
      }
      lastParentId = parent;
    });
    conversation = null;
    failure = null;
    if (!parent) return;
    let current = true;
    let made: SideConversation | null = null;
    openSideConversation(parent, { sideQuestions: () => sideQuestions, onDataChanged: (data) => onDataChanged(data) })
      .then((opened) => {
        if (!current) return opened.dispose();
        made = opened;
        opened.open(untrack(() => activeSideId));
        conversation = opened;
      })
      .catch((cause) => {
        if (current) failure = String(cause);
      });
    return () => {
      current = false;
      made?.dispose();
    };
  });

  // The selection is the parent's to bind (its Back button clears it) and the
  // conversation's to move (a first question opens what it created).
  $effect(() => {
    const target = conversation;
    const id = activeSideId;
    if (target) untrack(() => target.open(id));
  });
  $effect(() => {
    if (conversation) activeSideId = conversation.openId;
  });

  $effect(() => {
    const target = conversation;
    if (!target || !submission || submission.id === lastSubmission || target.loading) return;
    lastSubmission = submission.id;
    const text = submission.text;
    untrack(() => {
      question = text;
      if (!target.busy) void ask();
    });
  });

  async function ask() {
    const target = conversation;
    const text = question.trim();
    if (!target || !text || target.busy || target.loading) return;
    question = "";
    const asked = await target.ask(text);
    if (!asked && conversation === target && !question) question = text;
  }
</script>

{#if conversation}
  {@const transcript = conversation.transcript}
  <div class="min-h-0 flex-1 overflow-y-auto p-3">
    {#if conversation.about.pinned || (!conversation.openId && conversation.entries.length === 0)}
      <p class="mb-3 text-xs leading-5 text-surface-500">{conversation.about.text}</p>
    {/if}
    {#if !conversation.openId}
      <SideQuestionList
        entries={conversation.entries}
        onOpen={(entry) => conversation?.open(entry.sideThreadId)}
        onDelete={(entry) => void conversation?.remove(entry)}
      />
    {:else if conversation.loading}
      <div class="space-y-2" aria-label="Loading side question">
        <div class="placeholder h-12 animate-pulse rounded-lg"></div>
        <div class="placeholder h-20 animate-pulse rounded-lg opacity-70"></div>
      </div>
    {:else if transcript?.shape === "turns"}
      <SideTurns
        threadId={conversation.openId}
        thread={transcript.thread}
        turns={transcript.turns}
        activeTurn={transcript.activeTurn}
      />
    {:else if transcript}
      <SideExchanges exchanges={transcript.exchanges} />
    {/if}
    <!-- A turns transcript shows its own progress; anything else gets a line. -->
    {#if conversation.busy && transcript?.shape !== "turns"}
      <p class="mt-3 text-xs text-surface-500" role="status">Answering side question…</p>
    {/if}
    {#if conversation.error}
      <div class="error-note mt-3" role="alert">{conversation.error}</div>
    {/if}
  </div>

  <SideComposer
    bind:value={question}
    followUp={!!conversation.openId}
    busy={conversation.busy}
    disabled={conversation.loading}
    onAsk={() => void ask()}
    onStop={() => void conversation?.stop()}
  />
{:else}
  <p class="p-3 text-xs text-surface-500">
    {failure ?? (parentThreadId ? "Loading side questions…" : "Open a conversation to ask a side question.")}
  </p>
{/if}
