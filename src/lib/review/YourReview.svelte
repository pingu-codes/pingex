<!--
  The "Your review" panel: the pending review's entries with where they sit
  and where they came from, the summary, the verdict, and Submit, which
  names how many comments it posts. `agentReview` is where an agent
  review's summary card goes; Up next lists the PRs waiting on me.
-->
<script lang="ts">
  import { AlertTriangle, PanelRightClose, X } from "@lucide/svelte";
  import type { Snippet } from "svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import type { People } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";
  import type { PendingReviewStore } from "$lib/review/pendingReview.svelte";
  import { entryLabel } from "$lib/review/reviewPage";
  import type { InboxPr, PendingEntry, ReviewVerdict } from "$lib/types";

  let {
    pending,
    busy = false,
    error = null,
    stale = false,
    onSubmit,
    onJump,
    upNext,
    onOpenPr,
    agentReview,
    ownPr = false,
    people,
    onCollapse,
  }: {
    pending: PendingReviewStore;
    busy?: boolean;
    error?: string | null;
    /** The PR moved on since the entries were written. */
    stale?: boolean;
    onSubmit: () => void;
    /** Scroll to where an entry sits. */
    onJump: (entry: PendingEntry) => void;
    upNext: InboxPr[];
    onOpenPr?: (number: number) => void;
    agentReview?: Snippet;
    /** The viewer wrote the PR: GitHub only takes a comment from its author. */
    ownPr?: boolean;
    people?: People;
    /** Fold the panel away; without it the panel has no collapse button. */
    onCollapse?: () => void;
  } = $props();

  const VERDICTS = [
    { value: "comment", label: "Comment" },
    { value: "approve", label: "Approve" },
    { value: "requestChanges", label: "Request changes" },
  ] as const;

  const SOURCE: Record<PendingEntry["kind"], { label: string; tone: string }> = {
    comment: { label: "Draft", tone: "preset-tonal" },
    suggestion: { label: "Suggestion", tone: "preset-tonal" },
    reply: { label: "Reply", tone: "preset-tonal" },
    acceptedFinding: { label: "From agent", tone: "preset-tonal-primary" },
  };

  const count = $derived(pending.commentCount);
  const submitLabel = $derived(
    count === 0 ? "Submit review" : `Submit review with ${count} ${count === 1 ? "comment" : "comments"}`,
  );
  const canSubmit = $derived(
    !pending.loadError && (count > 0 || pending.summary.trim().length > 0 || (!ownPr && pending.verdict === "approve")),
  );
</script>

<aside aria-label="Your review" class="flex min-w-0 flex-col gap-3 text-xs">
  <div class="flex items-center justify-between gap-2">
    <div class="eyebrow">Your review</div>
    {#if onCollapse}
      <TooltipButton
        type="button"
        label="Collapse Your review"
        class="btn-icon btn-icon-sm hover:preset-tonal -my-1 text-surface-500"
        aria-expanded={true}
        onclick={onCollapse}
      >
        <PanelRightClose size={14} aria-hidden="true" />
      </TooltipButton>
    {/if}
  </div>
  {#if pending.entries.length > 0}
    <div class="panel-card flex flex-col p-1.5">
      {#each pending.entries as entry (entry.id)}
        <div class="group/entry flex items-center">
          <button type="button" class="menu-row min-w-0" onclick={() => onJump(entry)}>
            <span class="status-badge {SOURCE[entry.kind].tone}">{SOURCE[entry.kind].label}</span>
            <span class="font-mono text-[11px]">{entryLabel(entry)}</span>
            <span class="min-w-0 grow truncate text-surface-500">{entry.body}</span>
          </button>
          <button
            type="button"
            class="btn-icon btn-icon-sm hover:preset-tonal shrink-0 text-surface-500"
            aria-label="Remove {entryLabel(entry)} from the review"
            onclick={() => pending.remove(entry.id)}
          >
            <X size={12} aria-hidden="true" />
          </button>
        </div>
      {/each}
    </div>
  {:else}
    <p class="text-surface-500">Select lines in the diff to comment, or reply to a thread with Add to review.</p>
  {/if}

  {#if stale || pending.writtenAt}
    <div class="flex items-start gap-1.5 rounded-md preset-tonal-warning p-2">
      <AlertTriangle size={13} class="mt-px shrink-0" aria-hidden="true" />
      <span>The PR has new commits since you wrote these. They will post against the commit you read.</span>
    </div>
  {/if}

  <label for="review-summary" class="font-medium">Summary</label>
  <textarea
    id="review-summary"
    rows="4"
    placeholder="Overall comment"
    class="field w-full resize-none text-[13px]"
    value={pending.summary}
    oninput={(event) => pending.setSummary(event.currentTarget.value)}
    onblur={() => void pending.save().catch(() => {})}
  ></textarea>

  {#if !ownPr}
    <Segmented
      label="Verdict"
      vertical
      value={pending.verdict}
      options={VERDICTS}
      onChange={(verdict: ReviewVerdict) => pending.setVerdict(verdict)}
    />
  {/if}
  {#if pending.loadError}
    <div class="error-note flex flex-col items-start gap-1">
      <span>Couldn't read your saved review, so nothing here is being saved: {pending.loadError}</span>
      <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={() => pending.retryLoad()}>
        Try again
      </button>
    </div>
  {:else if pending.saveError}
    <div class="error-note">Your latest changes aren't saved: {pending.saveError}</div>
  {/if}
  {#if error}
    <div class="error-note">{error}</div>
  {/if}
  <button
    type="button"
    class="btn btn-sm preset-filled-primary-500 self-stretch text-xs"
    disabled={busy || !canSubmit}
    onclick={onSubmit}
    title="⌘↵"
  >
    {submitLabel}
  </button>

  {@render agentReview?.()}

  {#if upNext.length > 0}
    <div class="eyebrow mt-2">Up next</div>
    <div class="flex flex-col">
      {#each upNext as pr (pr.number)}
        <button type="button" class="menu-row" onclick={() => onOpenPr?.(pr.number)} disabled={!onOpenPr}>
          <LoginAvatar login={pr.author} person={people?.get(pr.author)} />
          <span class="font-mono text-[11px] text-surface-500">#{pr.number}</span>
          <span class="min-w-0 grow truncate">{pr.title}</span>
        </button>
      {/each}
    </div>
  {/if}
</aside>
