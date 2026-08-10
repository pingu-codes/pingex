<script lang="ts">
import { ArrowUp, MessageCircleQuestion, Square, Trash2 } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import {
  addSideQuestion,
  forkThread,
  interruptTurn,
  invalidateThreadCache,
  readThread,
  removeSideQuestion,
  startTurn,
} from "$lib/services/api";
import { type CodexEvent, setThreadHandler } from "$lib/services/codexEvents.svelte";
import { applyThreadEvent } from "$lib/thread/threadStream";
import { messageParts } from "$lib/thread/turnSegments";
import type { BootstrapData, SideQuestion, ThreadDetail } from "$lib/types";
import { renderMarkdown } from "$lib/utils/markdown";

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

let sideThread = $state<ThreadDetail | null>(null);
let sideLoading = $state(false);
let starting = $state(false);
let sideError = $state<string | null>(null);
let question = $state("");
let lastParentId: string | null | undefined;

const mine = $derived(sideQuestions.filter((entry) => entry.parentThreadId === parentThreadId));
const activeTurn = $derived(sideThread?.turns.find((turn) => turn.status === "inProgress") ?? null);
const busy = $derived(activeTurn !== null || starting);

// The panel can outlive a thread switch; a stale activeSideId would route the
// next question into the previous thread's side conversation.
$effect(() => {
  if (lastParentId === undefined || parentThreadId === lastParentId) {
    lastParentId = parentThreadId;
    return;
  }
  lastParentId = parentThreadId;
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
  sideLoading = true;
  sideError = null;
  readThread(id)
    .then((detail) => {
      if (id === activeSideId) sideThread = detail;
    })
    .catch((cause) => {
      if (id === activeSideId) sideError = cause instanceof Error ? cause.message : String(cause);
    })
    .finally(() => {
      if (id === activeSideId) sideLoading = false;
    });
});

$effect(() => setThreadHandler(handleEvent));

function handleEvent(event: CodexEvent) {
  const { method, params } = event;
  if (method === "disconnected") {
    for (const turn of sideThread?.turns ?? []) {
      if (turn.status === "inProgress") turn.status = "interrupted";
    }
    return;
  }
  if (!sideThread || !params || params.threadId !== activeSideId) return;
  const outcome = applyThreadEvent(sideThread, event);
  if (outcome.streamError) sideError = outcome.streamError;
  if (outcome.turnCompleted && activeSideId) invalidateThreadCache(activeSideId).catch(() => {});
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
      activeSideId = id;
      sideThread = { id, preview: text, cwd: "", turns: [] };
      onDataChanged(await addSideQuestion(parentThreadId, id, text));
    }
    sideThread?.turns.push({
      id: localTurnId,
      status: "inProgress",
      items: [{ type: "userMessage", id: `local-item-${Date.now()}`, content: [{ type: "text", text }] }],
    });
    const turn = await startTurn(id, [{ type: "text", text }]);
    const pending = sideThread?.turns.find((candidate) => candidate.id === localTurnId);
    if (pending) {
      pending.id = turn.id;
      pending.status = turn.status ?? "inProgress";
    }
  } catch (cause) {
    if (sideThread) sideThread.turns = sideThread.turns.filter((candidate) => candidate.id !== localTurnId);
    sideError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    starting = false;
  }
}

function stop() {
  if (!activeSideId || !activeTurn || activeTurn.id.startsWith("local-")) return;
  interruptTurn(activeSideId, activeTurn.id).catch((cause) => {
    sideError = cause instanceof Error ? cause.message : String(cause);
  });
}

async function deleteSide(entry: SideQuestion) {
  try {
    onDataChanged(await removeSideQuestion(entry.sideThreadId));
    if (activeSideId === entry.sideThreadId) {
      activeSideId = null;
      sideThread = null;
    }
  } catch (cause) {
    sideError = cause instanceof Error ? cause.message : String(cause);
  }
}

function openSide(entry: SideQuestion) {
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
    <div class="space-y-3">
      {#each sideThread.turns as turn (turn.id)}
        {#each turn.items as item (item.id)}
          {#if item.type === "userMessage"}
            <div class="flex justify-end">
              <div class="max-w-[90%] rounded-xl rounded-br-sm bg-primary-500/10 px-3 py-2 text-xs leading-5 whitespace-pre-wrap">
                {messageParts(item).map((part) => part.text ?? "").join("")}
              </div>
            </div>
          {:else if item.type === "agentMessage" || item.type === "plan"}
            <div class="prose-side text-xs leading-6">
              {@html renderMarkdown(item.text ?? "")}
            </div>
          {/if}
        {/each}
        {#if turn.status === "inProgress" && !turn.items.some((item) => item.type === "agentMessage")}
          <p class="text-xs text-surface-500">Thinking…</p>
        {/if}
      {/each}
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
    {:else}
      <TooltipButton
        label="Ask side question"
        onclick={ask}
        aria-label="Ask side question"
        disabled={!question.trim()}
        class="grid size-6 shrink-0 place-items-center rounded-full preset-filled-primary-500 disabled:opacity-40"
      >
        <ArrowUp size={12} />
      </TooltipButton>
    {/if}
  </div>
</div>

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
