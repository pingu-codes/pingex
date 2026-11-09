<!--
  One review thread inline in a file's diff: its comments, each with the
  comment menu, and a reply box. "Reply now" posts at once; "Add to review"
  holds the reply in the pending review until Submit.
-->
<script lang="ts" module>
  /** A thread on the PR that "Hand to agent" can send a comment to. */
  export type HandTarget = { id: string; label: string; hint?: string };
</script>

<script lang="ts">
  import { Ellipsis } from "@lucide/svelte";
  import { Avatar } from "@skeletonlabs/skeleton-svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import CommentBody from "$lib/review/CommentBody.svelte";
  import type { CommentThread } from "$lib/review/review";
  import { initials } from "$lib/review/reviewPage";
  import type { PrComment } from "$lib/types";

  let {
    thread,
    viewer,
    prUrl,
    busy = false,
    onReplyNow,
    onAddToReview,
    onResolve,
    onEdit,
    onDelete,
    onHandToAgent,
    onCopy,
    onOpenUrl,
    agentThreads = [],
  }: {
    thread: CommentThread;
    viewer: string;
    prUrl: string;
    busy?: boolean;
    /** Post a reply at once; resolves whether it was posted. */
    onReplyNow: (commentId: number, body: string) => Promise<boolean>;
    /** Hold a reply in the pending review. */
    onAddToReview: (body: string) => void;
    onResolve: () => void;
    onEdit: (comment: PrComment, body: string) => Promise<boolean>;
    onDelete: (comment: PrComment) => void;
    /** Hand one comment to an agent: a new thread (`askOnly` asks for no
     *  edits), or the existing thread `threadId`. */
    onHandToAgent: (comment: PrComment, askOnly: boolean, threadId?: string) => void;
    onCopy: (text: string) => void;
    onOpenUrl: (url: string) => void;
    /** Threads already on the PR, offered first under Hand to agent. */
    agentThreads?: HandTarget[];
  } = $props();

  let reply = $state("");
  let editing = $state<number | null>(null);
  let editBody = $state("");
  let replyBox = $state<HTMLTextAreaElement | null>(null);

  const location = $derived(`${thread.path}:${thread.line ?? "file"}`);
  const root = $derived(thread.comments[0]);
  const commentUrl = (comment: PrComment) => `${prUrl}#discussion_r${comment.id}`;

  function entries(comment: PrComment): MenuEntry[] {
    const own = comment.author === viewer;
    return [
      { id: "quote", label: "Quote reply", shortcut: "Q" },
      {
        kind: "submenu",
        id: "agent",
        label: "Hand to agent",
        entries: [
          ...agentThreads.map((target) => ({
            id: `agent-thread:${target.id}`,
            label: target.label,
            hint: target.hint,
          })),
          ...(agentThreads.length > 0 ? ([{ kind: "separator" }] satisfies MenuEntry[]) : []),
          { id: "agent-new", label: "New thread with this comment" },
          { id: "agent-ask", label: "Ask only, no edits" },
        ],
      },
      {
        id: "resolve",
        label: "Resolve thread",
        shortcut: "CmdOrCtrl+Enter",
        disabled: thread.resolved,
      },
      { id: "copy-link", label: "Copy link" },
      { id: "open", label: "Open on GitHub" },
      ...(own
        ? ([
            { kind: "separator" },
            { id: "edit", label: "Edit" },
            { id: "delete", label: "Delete", danger: true },
          ] satisfies MenuEntry[])
        : []),
    ];
  }

  function select(comment: PrComment, id: string) {
    if (id.startsWith("agent-thread:")) {
      onHandToAgent(comment, false, id.slice("agent-thread:".length));
      return;
    }
    switch (id) {
      case "quote":
        reply = `${comment.body
          .split("\n")
          .map((line) => `> ${line}`)
          .join("\n")}\n\n${reply}`;
        replyBox?.focus();
        break;
      case "agent-new":
        onHandToAgent(comment, false);
        break;
      case "agent-ask":
        onHandToAgent(comment, true);
        break;
      case "resolve":
        onResolve();
        break;
      case "copy-link":
        onCopy(commentUrl(comment));
        break;
      case "open":
        onOpenUrl(commentUrl(comment));
        break;
      case "edit":
        editing = comment.id;
        editBody = comment.body;
        break;
      case "delete":
        onDelete(comment);
        break;
    }
  }

  async function replyNow() {
    const body = reply.trim();
    if (!body || !root) return;
    if (await onReplyNow(root.id, body)) reply = "";
  }

  function addToReview() {
    const body = reply.trim();
    if (!body) return;
    onAddToReview(body);
    reply = "";
  }

  async function saveEdit(comment: PrComment) {
    const body = editBody.trim();
    if (!body) return;
    if (await onEdit(comment, body)) editing = null;
  }
</script>

<div
  role="group"
  aria-label="Thread on {location}"
  id="thread-{thread.key}"
  class="my-2 mr-3 ml-[104px] overflow-hidden rounded-container border border-surface-200-800 bg-surface-100-900 text-xs"
>
  {#each thread.comments as comment (comment.id)}
    <ActionMenu
      entries={entries(comment)}
      label="Comment by {comment.author}"
      onSelect={(id) => select(comment, id)}
      buttonLabel="Comment actions"
    >
      {#snippet row({ rowProps, buttonProps })}
        <div
          {...rowProps}
          class="group/comment flex flex-col gap-1 border-surface-200-800 p-2.5 outline-none not-first:border-t focus-visible:bg-surface-200-800/40"
        >
          <div class="flex items-center gap-1.5">
            <Avatar class="avatar"><Avatar.Fallback>{initials(comment.author)}</Avatar.Fallback></Avatar>
            <b class="font-medium">{comment.author}</b>
            {#if thread.resolved}
              <span class="status-badge preset-tonal-success">Resolved</span>
            {/if}
            <span class="grow"></span>
            <button {...buttonProps} type="button" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
              <Ellipsis size={14} aria-hidden="true" />
            </button>
          </div>
          {#if editing === comment.id}
            <label class="sr-only" for="edit-{comment.id}">Edit comment</label>
            <textarea
              id="edit-{comment.id}"
              bind:value={editBody}
              rows="3"
              class="field w-full resize-y text-[13px]"
            ></textarea>
            <div class="flex justify-end gap-1.5">
              <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={() => (editing = null)}>
                Cancel
              </button>
              <button
                type="button"
                class="btn btn-sm preset-tonal text-[11px]"
                disabled={busy}
                onclick={() => saveEdit(comment)}
              >
                Save
              </button>
            </div>
          {:else}
            <CommentBody class="pl-6" body={comment.body} />
          {/if}
        </div>
      {/snippet}
    </ActionMenu>
  {/each}
  <div class="flex flex-col gap-2 border-t border-surface-200-800 px-2.5 py-2">
    <label class="sr-only" for="reply-{thread.key}">Reply</label>
    <textarea
      id="reply-{thread.key}"
      bind:this={replyBox}
      bind:value={reply}
      rows="2"
      placeholder="Reply…"
      class="field w-full resize-none text-[13px]"
    ></textarea>
    <div class="flex flex-wrap items-center gap-1.5">
      {#if !thread.resolved}
        <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" disabled={busy} onclick={onResolve}>
          Resolve
        </button>
      {/if}
      <span class="grow"></span>
      <button
        type="button"
        class="btn btn-sm hover:preset-tonal text-[11px]"
        disabled={busy || !reply.trim()}
        onclick={replyNow}
      >
        Reply now
      </button>
      <button type="button" class="btn btn-sm preset-tonal text-[11px]" disabled={!reply.trim()} onclick={addToReview}>
        Add to review
      </button>
    </div>
  </div>
</div>
