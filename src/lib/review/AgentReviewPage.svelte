<!--
  The review page with agent reviews: the PR's Findings inline as drafts
  only the reviewer sees (`lineAnnotations`), their counts in the file tree
  and the Agent findings filter, the agent review summary in Your review,
  and Ask agent / Hand to agent answered in a thread filed under the PR.
-->
<script lang="ts">
  import { onMount } from "svelte";
  import { projects, threadById } from "$lib/app/appData.svelte";
  import { openThreadInCwd, type PrMove } from "$lib/app/navigation.svelte";
  import { providePageThread } from "$lib/app/pageThread";
  import {
    defaultReviewHarness,
    refreshAgentReviews,
    rerunAgentReview,
    reviewTarget,
  } from "$lib/review/agentReviews.svelte";
  import type { LineSlot } from "$lib/review/FileDiff.svelte";
  import FindingCard from "$lib/review/FindingCard.svelte";
  import { FindingsStore } from "$lib/review/findings.svelte";
  import PrWorktreeActions from "$lib/review/PrWorktreeActions.svelte";
  import { PendingReviewStore } from "$lib/review/pendingReview.svelte";
  import { ensurePrLinks, prLinks } from "$lib/review/prLinks.svelte";
  import { checkoutOf, readOnly, startPrThread } from "$lib/review/prOnDisk";
  import ReviewPage from "$lib/review/ReviewPage.svelte";
  import type { HandTarget } from "$lib/review/ReviewThread.svelte";
  import { findingTexts } from "$lib/review/reviewFind";
  import { type AskAgentRequest, askAgentPrompt, diffRows } from "$lib/review/reviewPage";
  import ThreadBranchChip from "$lib/review/ThreadBranchChip.svelte";
  import { startTurn } from "$lib/services/api";
  import { activeTurns } from "$lib/services/codexEvents.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { query } from "$lib/services/queryCache.svelte";
  import { toaster, toastFailure } from "$lib/toaster";
  import type { AgentReview, CheckoutTarget, Finding, Inbox, PrDetail } from "$lib/types";
  import { harnessName } from "$lib/utils/capabilities";
  import { plural, relativeTime } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";

  let {
    repoDir,
    number,
    onBack,
    backLabel,
    inbox = null,
    viewer = null,
    onOpenPr,
    onRunLocally,
    onWorkOnPr,
    onCheckOut,
    onPushToPr,
  }: {
    repoDir: string;
    number: number;
    /** The breadcrumb's way back; without it the page shows no breadcrumb. */
    onBack?: () => void;
    /** Names where `onBack` leads. */
    backLabel?: string;
    inbox?: Inbox | null;
    /** Who is reviewing: whether the PR is mine. */
    viewer?: string | null;
    onOpenPr?: (number: number, move?: PrMove) => void;
    onRunLocally?: (detail: PrDetail, action: "runLocally" | "openInEditor") => void;
    /** Work on this PR: a draft thread in its worktree. */
    onWorkOnPr?: (detail: PrDetail) => void;
    /** Check the PR out (its stack, for a stack's PR). */
    onCheckOut?: () => void;
    /** Push the worktree's commits to the PR. */
    onPushToPr?: (detail: PrDetail) => void;
  } = $props();

  // svelte-ignore state_referenced_locally -- the page is keyed by PR.
  const pending = new PendingReviewStore(repoDir, number);
  // svelte-ignore state_referenced_locally -- likewise.
  const findings = new FindingsStore(repoDir, number, pending);

  // The same reads as the page's: one entry each, so no second request.
  const detailQuery = query("reviewPrDetail", () => [repoDir, number]);
  const detail = $derived<PrDetail | null>(detailQuery.value ?? null);

  const checkoutQuery = checkoutOf(
    () => repoDir,
    () => number,
  );
  /** Undefined until answered, and when the read fails: the page then says
   *  nothing about a worktree rather than that there isn't one. */
  const checkout = $derived(checkoutQuery.value);
  // "New thread" here is Work on this PR: a draft in the PR's worktree, once
  // it is checked out.
  $effect(() => {
    const shown = detail;
    const work = onWorkOnPr;
    if (!shown || !work || !checkout) return;
    return providePageThread("pr", () => work(shown));
  });
  const repoName = $derived(basename(repoDir));
  const latest = $derived(findings.latest);
  const anyRun = $derived(findings.runs.length > 0 || findings.findings.length > 0);

  /** Every (path, side, line) a diff row draws, so Findings elsewhere can
   *  show in the summary instead of nowhere. */
  const drawn = $derived(
    new Set(
      (detail?.files ?? []).flatMap((file) =>
        diffRows(file).flatMap((row) => (row.kind === "line" ? [`${file.path}|${row.side}|${row.number}`] : [])),
      ),
    ),
  );
  const outside = $derived(
    detail
      ? findings.shown.filter(
          (finding) =>
            finding.state !== "merged" &&
            !drawn.has(`${finding.path}|${finding.side}|${finding.endLine ?? finding.line}`),
        )
      : [],
  );

  const threadLinks = $derived(
    Object.values(prLinks.byThread).filter((link) => link.prNumber === number && link.repoKey === repoDir),
  );
  const prThreads = $derived<HandTarget[]>(
    threadLinks.map((link) => ({
      id: link.threadId,
      label: threadById(link.threadId)?.title || `Thread ${link.threadId.slice(0, 8)}`,
      hint: activeTurns.list.includes(link.threadId) ? "running" : undefined,
    })),
  );

  onMount(() => {
    void refreshAgentReviews(repoDir).catch(() => []);
    void findings.load();
    void ensurePrLinks();
  });

  // A run on this PR finished: read its Findings.
  let seenFinish = "";
  $effect(() => {
    const key = latest ? `${latest.id}:${latest.status}:${latest.findings}` : "";
    if (key && key !== seenFinish) {
      const reload = seenFinish !== "" && latest?.status === "done";
      seenFinish = key;
      if (reload) void findings.load().then(() => findings.adoptAdded());
    }
  });

  const targetOf = (pr: PrDetail): CheckoutTarget => reviewTarget(repoDir, { ...pr.summary, headSha: pr.headSha });

  function openThread(threadId: string) {
    const link = prLinks.byThread[threadId];
    openThreadInCwd(threadId, link?.checkoutPath ?? repoDir);
  }

  /** Ask agent and Hand to agent: an existing thread on the PR, or a new
   *  one filed under it ("Ask only" runs read-only). */
  async function askAgent(request: AskAgentRequest) {
    const text = askAgentPrompt(request);
    try {
      if (request.threadId) {
        await startTurn(request.threadId, [{ type: "text", text }], request.askOnly ? readOnly(undefined) : undefined);
        openThread(request.threadId);
        return;
      }
      const { thread } = await startPrThread(repoDir, number, {
        prompt: [{ type: "text", text }],
        harness: defaultReviewHarness(),
        options: request.askOnly ? readOnly(undefined) : undefined,
        projects,
      });
      openThread(thread.id);
    } catch (cause) {
      toastFailure(cause);
    }
  }

  function runOf(finding: Finding): AgentReview | null {
    return findings.runs.find((run) => run.id === finding.agentReviewId) ?? null;
  }

  async function askWhy(finding: Finding) {
    const run = runOf(finding);
    if (!run?.threadId) return;
    const where = `${finding.path}:${finding.line}`;
    const text = `Why did you raise "${finding.title}" at ${where}? Show the evidence, and say how sure you are.`;
    try {
      await startTurn(run.threadId, [{ type: "text", text }]);
      openThread(run.threadId);
    } catch (cause) {
      toastFailure(cause);
    }
  }

  async function guard(work: () => Promise<unknown>) {
    await work().catch(toastFailure);
  }

  async function skipKind(rule: string) {
    await guard(async () => {
      await commands.reviewAddRule(repoDir, rule);
      toaster.success({ title: `Agent reviews in ${repoName} will skip this kind of finding` });
    });
  }

  function sameAs(finding: Finding): string | null {
    const comment = detail?.comments.find((entry) => entry.id === finding.sameAsCommentId);
    if (!comment) return null;
    const place = comment.path ? ` ${basename(comment.path)}${comment.line ? `:${comment.line}` : ""}` : "";
    return `${comment.author}'s${place}`;
  }

  async function rerun() {
    if (!latest || !detail) return;
    await guard(() => rerunAgentReview(latest, [targetOf(detail as PrDetail)]));
  }

  function age(run: AgentReview): string {
    const at = run.finishedAt ?? run.startedAt ?? run.createdAt;
    return relativeTime(at / 1000, { granularity: "minute" });
  }
</script>

{#snippet threadsTab()}
  <div class="flex flex-col gap-1">
    {#each prThreads as thread, index (thread.id)}
      <div class="flex items-center gap-2">
        <button type="button" class="menu-row min-w-0 grow" onclick={() => openThread(thread.id)}>
          <span class="min-w-0 grow truncate">{thread.label}</span>
          {#if thread.hint}
            <span class="text-xs text-surface-500">{thread.hint}</span>
          {/if}
        </button>
        <ThreadBranchChip link={threadLinks[index]} />
      </div>
    {:else}
      <p class="empty-state">Threads started on this PR show here, with their git branch and commit.</p>
    {/each}
  </div>
{/snippet}

{#snippet headerActions()}
  <PrWorktreeActions
    {checkout}
    known={checkout !== undefined}
    mine={inbox?.triage.find((entry) => entry.number === number)?.mine ?? false}
    {onCheckOut}
    onWork={detail && onWorkOnPr ? () => detail && onWorkOnPr(detail) : undefined}
    onPush={detail && onPushToPr ? () => detail && onPushToPr(detail) : undefined}
  />
{/snippet}

<ReviewPage
  {repoDir}
  {number}
  {onBack}
  {backLabel}
  {inbox}
  {onOpenPr}
  onAskAgent={askAgent}
  {onRunLocally}
  checkout={checkout ?? null}
  checkoutKnown={checkout !== undefined}
  {headerActions}
  {pending}
  {prThreads}
  {threadsTab}
  findingCounts={anyRun ? findings.counts : undefined}
  lineAnnotations={annotations}
  annotatedLines={findings.linesByPath}
  annotationText={(slot) =>
    findings.at(slot.path, slot.side, slot.line).flatMap((finding) =>
      findingTexts(finding, {
        agentLabel: harnessName(runOf(finding)?.request.harness),
        added: findings.isAdded(finding),
        addedBody: findings.entryFor(finding)?.body ?? null,
        sameAs: sameAs(finding),
      }),
    )}
  agentReview={summary}
/>

{#snippet card(finding: Finding, outsideDiff: boolean)}
  {@const entry = findings.entryFor(finding)}
  <FindingCard
    {finding}
    agentLabel={harnessName(runOf(finding)?.request.harness)}
    {repoName}
    added={findings.isAdded(finding)}
    addedBody={entry?.body ?? null}
    sameAs={sameAs(finding)}
    compact={outsideDiff}
    onAdd={(body) => void guard(() => findings.add(finding, body))}
    onRemove={() => findings.removeFromReview(finding)}
    onAskWhy={runOf(finding)?.threadId ? () => void askWhy(finding) : undefined}
    onDismiss={(reason, note) => void guard(() => findings.dismiss(finding, reason, note ?? null))}
    onSkipKind={(rule) => void skipKind(rule)}
  />
{/snippet}

{#snippet annotations(slot: LineSlot)}
  {#each findings.at(slot.path, slot.side, slot.line) as finding (finding.id)}
    {@render card(finding, false)}
  {/each}
{/snippet}

{#snippet summary()}
  {#if latest}
    {@const tally = findings.tally}
    <div class="eyebrow mt-2">Agent review</div>
    <section aria-label="Agent review" class="panel-card flex flex-col gap-1.5 px-3 py-2.5 text-xs">
      <div class="flex items-center gap-2">
        <span class="grow">{harnessName(latest.request.harness)} · in the worktree</span>
        <span class="text-surface-500">{age(latest)}</span>
      </div>
      <div class="text-surface-500">
        {#if latest.status === "queued"}
          Queued
        {:else if latest.status === "running"}
          Running…
        {:else if latest.status === "failed"}
          {latest.error ?? "The agent review stopped"}
        {:else if latest.parseError}
          No findings parsed
        {:else}
          {plural(tally.total, "finding")}
          · {tally.added} added to your review · {tally.dismissed} dismissed
        {/if}
      </div>
      <div class="flex flex-wrap gap-1.5">
        {#if latest.threadId}
          <button
            type="button"
            class="btn btn-sm hover:preset-tonal text-[11px]"
            onclick={() => openThread(latest.threadId as string)}
          >
            Open thread
          </button>
        {/if}
        {#if latest.status === "done" || latest.status === "failed"}
          <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={rerun}>
            Re-run on new commits
          </button>
        {/if}
      </div>
    </section>
    {#if outside.length > 0}
      <div class="eyebrow mt-2">Findings off the changed lines</div>
      {#each outside as finding (finding.id)}
        {@render card(finding, true)}
      {/each}
    {/if}
  {/if}
{/snippet}
