<script lang="ts">
import { Bot, Check, CheckCheck, MessageSquare, Reply, Send, Trash2 } from "@lucide/svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { type CommentThread, commentThreads, conversationComments } from "$lib/review/review";
import type { PendingComment, PrComment } from "$lib/types";

let {
  comments = [],
  pending = [],
  reviewStarted = false,
  busy = false,
  onStartReview,
  onSubmit,
  onReply,
  onResolve,
  onRemovePending,
  onAskCodex,
}: {
  comments?: PrComment[];
  pending?: PendingComment[];
  reviewStarted?: boolean;
  busy?: boolean;
  onStartReview: () => void;
  onSubmit: (event: string, body: string) => void;
  onReply: (commentId: number, body: string) => void;
  onResolve: (threadId: string) => void;
  onRemovePending: (index: number) => void;
  onAskCodex: () => void;
} = $props();

const threads = $derived<CommentThread[]>(commentThreads(comments));
const conversation = $derived<PrComment[]>(conversationComments(comments));

let reviewEvent = $state<"comment" | "approve" | "request-changes">("comment");
let reviewBody = $state("");

// Per-thread reply drafts, keyed by thread key.
let replyOpen = $state<string | null>(null);
let replyBody = $state("");

function lastCommentId(thread: CommentThread): number {
  return thread.comments[thread.comments.length - 1]?.id ?? 0;
}

function submitReply(thread: CommentThread) {
  const body = replyBody.trim();
  const commentId = lastCommentId(thread);
  if (!body || !commentId) return;
  onReply(commentId, body);
  replyBody = "";
  replyOpen = null;
}

function submitReview() {
  const body = reviewBody.trim();
  // Approve may carry an empty body; comment / request-changes need one.
  if (reviewEvent !== "approve" && !body) return;
  onSubmit(reviewEvent, body);
}
</script>

<div class="flex h-full flex-col">
  <div class="flex items-center justify-between border-b border-surface-200-800 px-4 py-3">
    <h2 class="flex items-center gap-1.5 text-sm font-semibold">
      <MessageSquare size={15} class="text-surface-500" /> Review
    </h2>
    <button onclick={onAskCodex} disabled={busy} class="btn btn-sm preset-tonal-primary" title="Open a Codex thread with the PR diff">
      <Bot size={14} /> Ask Codex
    </button>
  </div>

  <div class="min-h-0 flex-1 space-y-4 overflow-y-auto px-4 py-3">
    <!-- Pending (unsubmitted) inline comments -->
    {#if pending.length > 0}
      <section>
        <h3 class="text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">
          Pending comments
          <span class="ml-1 rounded-full bg-primary-500/15 px-1.5 py-0.5 text-[10px] text-primary-500">{pending.length}</span>
        </h3>
        <ul class="mt-2 space-y-2">
          {#each pending as item, index (index)}
            <li class="rounded-lg border border-primary-500/30 bg-primary-500/5 p-2.5 text-xs">
              <div class="flex items-center justify-between gap-2">
                <span class="truncate font-mono text-[10px] text-surface-500">{item.path}:{item.line} · {item.side}</span>
                <TooltipButton label="Remove pending comment" onclick={() => onRemovePending(index)} aria-label="Remove pending comment" class="btn-icon btn-icon-sm text-surface-500 hover:preset-tonal">
                  <Trash2 size={12} />
                </TooltipButton>
              </div>
              <p class="mt-1 whitespace-pre-wrap leading-5">{item.body}</p>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <!-- Existing inline comment threads -->
    <section>
      <h3 class="text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Comment threads</h3>
      {#if threads.length === 0}
        <p class="mt-2 rounded-lg border border-dashed border-surface-300-700 px-3 py-4 text-center text-xs text-surface-500">
          No inline comments yet.
        </p>
      {:else}
        <ul class="mt-2 space-y-2">
          {#each threads as thread (thread.key)}
            <li class="rounded-lg border border-surface-200-800 bg-surface-100-900 p-2.5" data-testid="thread">
              <div class="flex items-center justify-between gap-2">
                <span class="truncate font-mono text-[10px] text-surface-500">{thread.path}{thread.line != null ? `:${thread.line}` : ""}</span>
                {#if thread.resolved}
                  <span class="inline-flex items-center gap-0.5 rounded-full bg-success-500/15 px-1.5 py-0.5 text-[10px] font-medium text-success-600 dark:text-success-400">
                    <CheckCheck size={10} /> resolved
                  </span>
                {/if}
              </div>
              <ul class="mt-1.5 space-y-1.5">
                {#each thread.comments as comment (comment.id)}
                  <li class="text-xs">
                    <span class="font-semibold">{comment.author}</span>
                    <p class="mt-0.5 whitespace-pre-wrap leading-5 text-surface-700-300">{comment.body}</p>
                  </li>
                {/each}
              </ul>
              <div class="mt-2 flex items-center gap-1">
                <button onclick={() => { replyOpen = replyOpen === thread.key ? null : thread.key; replyBody = ""; }} class="btn btn-sm preset-tonal">
                  <Reply size={13} /> Reply
                </button>
                {#if !thread.resolved && !thread.key.includes(":")}
                  <!-- A colon in the key means it was synthesized from path:line
                       (no GraphQL node id), so it cannot be resolved. -->
                  <button onclick={() => onResolve(thread.key)} disabled={busy} class="btn btn-sm preset-tonal" title="Resolve this thread">
                    <Check size={13} /> Resolve
                  </button>
                {/if}
              </div>
              {#if replyOpen === thread.key}
                <div class="mt-2">
                  <textarea
                    bind:value={replyBody}
                    aria-label="Reply"
                    rows="2"
                    placeholder="Write a reply…"
                    class="w-full resize-y rounded-md border border-surface-300-700 bg-surface-50-950 p-2 text-xs"
                  ></textarea>
                  <div class="mt-1 flex justify-end">
                    <button onclick={() => submitReply(thread)} disabled={busy || !replyBody.trim()} class="btn btn-sm preset-filled-primary-500">
                      <Send size={13} /> Send reply
                    </button>
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <!-- Conversation comments -->
    {#if conversation.length > 0}
      <section>
        <h3 class="text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Conversation</h3>
        <ul class="mt-2 space-y-2">
          {#each conversation as comment (comment.id + comment.createdAt)}
            <li class="rounded-lg border border-surface-200-800 bg-surface-100-900 p-2.5 text-xs">
              <span class="font-semibold">{comment.author}</span>
              <p class="mt-0.5 whitespace-pre-wrap leading-5 text-surface-700-300">{comment.body}</p>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>

  <!-- Submit review -->
  <div class="border-t border-surface-200-800 px-4 py-3">
    {#if !reviewStarted}
      <button onclick={onStartReview} class="btn btn-sm w-full preset-tonal-primary">
        Start review
      </button>
    {:else}
      <div class="space-y-2">
        <textarea
          bind:value={reviewBody}
          aria-label="Review summary"
          rows="2"
          placeholder="Overall review comment…"
          class="w-full resize-y rounded-md border border-surface-300-700 bg-surface-50-950 p-2 text-xs"
        ></textarea>
        <div class="flex items-center gap-2">
          <select bind:value={reviewEvent} aria-label="Review action" class="select select-sm flex-1 text-xs">
            <option value="comment">Comment</option>
            <option value="approve">Approve</option>
            <option value="request-changes">Request changes</option>
          </select>
          <button onclick={submitReview} disabled={busy || (reviewEvent !== "approve" && !reviewBody.trim())} class="btn btn-sm preset-filled-primary-500">
            <Send size={13} /> Submit review
          </button>
        </div>
        {#if pending.length > 0}
          <p class="text-[11px] text-surface-500">{pending.length} pending comment{pending.length === 1 ? "" : "s"} will be included.</p>
        {/if}
      </div>
    {/if}
  </div>
</div>
