<!--
  The review page's Conversation tab: the PR's description, its comments,
  reviews and threads in order, and its commits. `threadCard` is the page's
  thread card, the same one the Files tab shows.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import type { People } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";
  import CommentBody from "$lib/review/CommentBody.svelte";
  import { type CommentThread, type conversationTimeline, reviewVerb } from "$lib/review/review";
  import ThreadContext from "$lib/review/ThreadContext.svelte";
  import type { PrDetail } from "$lib/types";

  let {
    pr,
    conversation,
    people,
    onJump,
    threadCard,
  }: {
    pr: PrDetail;
    conversation: ReturnType<typeof conversationTimeline>;
    people: People;
    /** Go to a thread's file on the Files tab. */
    onJump: (path: string) => void;
    threadCard: Snippet<[CommentThread]>;
  } = $props();
</script>

{#snippet conversationThread(found: CommentThread)}
  <!-- Away from the diff, a thread shows where it is and the lines it is about. -->
  <div class="flex flex-col gap-1">
    <ThreadContext thread={found} {onJump} />
    {@render threadCard(found)}
  </div>
{/snippet}

<div class="flex max-w-3xl flex-col gap-3">
  {#if pr.body.trim()}
    <CommentBody class="panel-card p-3" body={pr.body} />
  {/if}
  {#each conversation as entry (entry.key)}
    {#if entry.kind === "comment"}
      <div class="panel-card flex flex-col gap-1 p-2.5">
        <div class="flex items-center gap-1.5">
          <LoginAvatar login={entry.comment.author} person={people.get(entry.comment.author)} />
          <b class="font-medium">{entry.comment.author}</b>
          <span class="text-surface-500">{new Date(entry.comment.createdAt).toLocaleString()}</span>
        </div>
        <CommentBody class="pl-6" body={entry.comment.body} />
      </div>
    {:else if entry.kind === "review"}
      {@const review = entry.review}
      <div class="panel-card flex flex-col gap-2 p-2.5">
        <div class="flex items-center gap-1.5">
          <LoginAvatar login={review.author} person={people.get(review.author)} />
          <b class="font-medium">{review.author}</b>
          {#if review.state === "APPROVED"}
            <span class="status-badge preset-tonal-success">{reviewVerb(review.state)}</span>
          {:else if review.state === "CHANGES_REQUESTED"}
            <span class="status-badge preset-tonal-warning">{reviewVerb(review.state)}</span>
          {:else}
            <span class="text-surface-500">{reviewVerb(review.state)}</span>
          {/if}
          <span class="text-surface-500">{new Date(review.submittedAt).toLocaleString()}</span>
        </div>
        {#if review.body.trim()}
          <CommentBody class="pl-6" body={review.body} />
        {/if}
        {#each entry.threads as found (found.key)}
          {@render conversationThread(found)}
        {/each}
      </div>
    {:else}
      {@render conversationThread(entry.thread)}
    {/if}
  {:else}
    <p class="empty-state">No conversation yet.</p>
  {/each}
  {#if pr.commits.length > 0}
    <div class="eyebrow mt-2">Commits</div>
    <div class="panel-card flex flex-col p-1.5">
      {#each pr.commits as commit (commit.oid)}
        <div class="menu-row">
          <span class="font-mono text-[11px] text-surface-500">{commit.shortOid}</span>{commit.headline}
        </div>
      {/each}
    </div>
  {/if}
</div>
