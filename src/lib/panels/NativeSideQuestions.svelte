<script lang="ts">
import { ArrowUp, MessageCircleQuestion, Square, Trash2 } from "@lucide/svelte";
import { onDestroy, untrack } from "svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { commands, identity, scopedId } from "$lib/services/homeRouting";
import type { BootstrapData, SideExchange, SideQuestion } from "$lib/types";
import { renderMarkdown } from "$lib/utils/markdown";

let {
  parentThreadId,
  sideQuestions,
  activeSideId = $bindable(null),
  onDataChanged,
  submission = null,
}: {
  parentThreadId: string;
  sideQuestions: SideQuestion[];
  activeSideId?: string | null;
  onDataChanged: (data: BootstrapData) => void;
  submission?: { id: number; text: string } | null;
} = $props();

let question = $state("");
let exchanges = $state<SideExchange[]>([]);
let error = $state<string | null>(null);
let loading = $state(false);
let starting = $state(false);
let startingSideId = $state<string | null>(null);
let mounted = true;
let readRevision = 0;
let lastSubmission: number | undefined;
let flight: { cancelled: boolean; prepared: Promise<{ id: string; request: string }> } | null = null;
const mine = $derived(
  sideQuestions.filter((entry) => entry.parentThreadId === parentThreadId && entry.kind === "native"),
);
const pending = $derived(exchanges.find((entry) => entry.outcome === "pending"));
const startingHere = $derived(starting && (!activeSideId || activeSideId === startingSideId));
const busy = $derived(startingHere || !!pending);
onDestroy(() => {
  mounted = false;
});

$effect(() => {
  const id = activeSideId;
  readRevision++;
  loading = false;
  exchanges = [];
  error = null;
  if (id)
    untrack(() => {
      void load(id);
    });
});

// Poll one read at a time. Each `load` supersedes the one before it, so on a
// fixed interval a read slower than the interval was always stale on arrival
// and the answer never showed.
$effect(() => {
  if (!pending || startingHere || !activeSideId) return;
  const id = activeSideId;
  let stopped = false;
  let timer: ReturnType<typeof setTimeout>;
  const poll = async () => {
    await load(id, false);
    if (!stopped) timer = setTimeout(poll, 1000);
  };
  timer = setTimeout(poll, 1000);
  return () => {
    stopped = true;
    clearTimeout(timer);
  };
});

$effect(() => {
  if (!submission || submission.id === lastSubmission || loading) return;
  lastSubmission = submission.id;
  const text = submission.text;
  untrack(() => {
    question = text;
    if (!busy) void ask();
  });
});

async function load(id: string, showLoading = true) {
  const revision = ++readRevision;
  if (showLoading) loading = true;
  try {
    const result = await commands.readNativeSideQuestion(parentThreadId, id);
    if (mounted && activeSideId === id && revision === readRevision) exchanges = result;
  } catch (cause) {
    if (mounted && activeSideId === id && revision === readRevision) error = String(cause);
  } finally {
    if (mounted && activeSideId === id && revision === readRevision) loading = false;
  }
}

async function ask() {
  const text = question.trim();
  if (!text || busy || loading) return;
  const parent = parentThreadId;
  const selectionAtStart = activeSideId;
  const selected = activeSideId && mine.some((entry) => entry.sideThreadId === activeSideId) ? activeSideId : null;
  let originId = selected;
  question = "";
  error = null;
  starting = true;
  startingSideId = selected;
  readRevision++;
  const operation = {
    cancelled: false,
    prepared: (async () => {
      let id = selected;
      if (!id) {
        const nativeId = crypto.randomUUID();
        const home = identity(parent)?.homeKey;
        id = home ? scopedId(home, nativeId) : nativeId;
        originId = id;
        startingSideId = id;
        const data = await commands.addNativeSideQuestion(parent, id, text);
        onDataChanged(data);
        if (mounted && activeSideId === selectionAtStart) activeSideId = id;
      }
      const request = await commands.prepareNativeSideQuestion(parent, id, text);
      return { id, request };
    })(),
  };
  flight = operation;
  let prepared: { id: string; request: string } | null = null;
  try {
    const { id, request } = await operation.prepared;
    prepared = { id, request };
    if (mounted && activeSideId === id) {
      readRevision++;
      loading = false;
      exchanges = [
        ...exchanges,
        {
          id: request,
          question: text,
          response: null,
          outcome: "pending",
          synthetic: false,
          refusal: null,
          createdAt: Date.now() / 1000,
        },
      ];
    }
    const result = operation.cancelled
      ? await commands.cancelNativeSideQuestion(parent, id, request)
      : await commands.answerNativeSideQuestion(parent, id, request);
    if (mounted && activeSideId === id) {
      readRevision++;
      exchanges = result;
      if (result.find((entry) => entry.id === request)?.outcome === "error" && !question) question = text;
    }
  } catch (cause) {
    if (prepared) {
      await commands.cancelNativeSideQuestion(parent, prepared.id, prepared.request).catch(() => {});
    }
    if (mounted && (!activeSideId || activeSideId === (prepared?.id ?? originId))) {
      readRevision++;
      exchanges = exchanges.map((entry) =>
        entry.id === prepared?.request ? { ...entry, outcome: "error", response: String(cause) } : entry,
      );
      error = String(cause);
      if (!question) question = text;
    }
  } finally {
    if (flight === operation) {
      flight = null;
      if (mounted) starting = false;
    }
  }
}

async function stop() {
  const parent = parentThreadId;
  const operation = startingHere ? flight : null;
  if (operation) operation.cancelled = true;
  try {
    const target = operation
      ? await operation.prepared
      : pending && activeSideId
        ? { id: activeSideId, request: pending.id }
        : null;
    if (!target) return;
    const result = await commands.cancelNativeSideQuestion(parent, target.id, target.request);
    if (mounted && activeSideId === target.id) {
      readRevision++;
      exchanges = result;
    }
  } catch (cause) {
    if (mounted) error = String(cause);
  }
}

async function remove(entry: SideQuestion) {
  try {
    onDataChanged(await commands.removeSideQuestion(entry.sideThreadId));
    if (mounted && activeSideId === entry.sideThreadId) activeSideId = null;
  } catch (cause) {
    if (mounted) error = String(cause);
  }
}
</script>

<div class="min-h-0 flex-1 overflow-y-auto p-3">
  <p class="mb-3 text-xs leading-5 text-surface-500">Ask about what Claude already knows from this conversation. Side questions cannot read files or use tools, and stay out of the main conversation.</p>
  {#if !activeSideId}
    <div class="space-y-1">
      {#each mine as entry (entry.sideThreadId)}
        <div class="flex items-center gap-1">
          <button class="flex min-w-0 flex-1 items-center gap-2 rounded-lg px-2.5 py-2 text-left text-xs hover:preset-tonal" onclick={() => (activeSideId = entry.sideThreadId)}>
            <MessageCircleQuestion size={13} /><span class="truncate">{entry.title}</span>
          </button>
          <TooltipButton label="Delete side question" aria-label={`Delete side question ${entry.title}`} onclick={() => remove(entry)} class="btn-icon btn-icon-sm"><Trash2 size={12} /></TooltipButton>
        </div>
      {/each}
    </div>
  {:else if loading}
    <p class="text-xs text-surface-500" aria-label="Loading side question">Loading…</p>
  {:else}
    <div class="space-y-3 text-xs" aria-live="polite">
      {#each exchanges as entry (entry.id)}
        <div class="ml-auto max-w-[90%] rounded-xl bg-primary-500/10 px-3 py-2 whitespace-pre-wrap">{entry.question}</div>
        {#if entry.refusal}<p class="text-warning-500">{entry.refusal.content}</p>{/if}
        {#if entry.outcome === "interrupted"}
          <p class="text-surface-500">Side question stopped.</p>
        {:else if entry.response}
          <div class:text-error-500={entry.outcome === "error"} class="markdown-body text-xs">{@html renderMarkdown(entry.response)}</div>
        {/if}
      {/each}
    </div>
  {/if}
  {#if busy}<p class="mt-3 text-xs text-surface-500" role="status">Answering side question…</p>{/if}
  {#if error}<p class="mt-3 text-xs text-error-500" role="alert">{error}</p>{/if}
</div>
<div class="border-t border-surface-200-800 p-2">
  <div class="flex items-end gap-2 rounded-xl border border-surface-200-800 px-2.5 py-1.5">
    <textarea bind:value={question} rows="1" placeholder={activeSideId ? "Follow up…" : "Ask a side question…"}
      class="max-h-24 min-h-[1.25rem] flex-1 resize-none bg-transparent text-xs leading-5 outline-none"
      onkeydown={(event) => {
        if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); void ask(); }
        else if (event.key === "Escape" && busy) { event.preventDefault(); void stop(); }
      }}></textarea>
    {#if busy}<TooltipButton label="Stop (Esc)" aria-label="Stop side question" onclick={stop} class="btn-icon btn-icon-sm preset-filled-error-500"><Square size={10} /></TooltipButton>{/if}
    <TooltipButton label="Ask side question" aria-label="Ask side question" onclick={ask} disabled={busy || loading || !question.trim()} class="btn-icon btn-icon-sm preset-filled-primary-500"><ArrowUp size={12} /></TooltipButton>
  </div>
</div>
