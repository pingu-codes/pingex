// The agent review queue, as the app runs it. The desk keeps each run and
// decides which may start (two at a time across the Home); this module
// starts a claimed run as a thread filed under its PR (`startPrThread`),
// watches for the end of its first turn, and hands the final message back
// so the desk can parse its Findings. Every screen reads `agentReviews`.
//
// Hooks for other screens:
//   enqueueAgentReview(repoDir, request)  what Start review calls
//   startStackReview(repoDir, prs)        "Review whole stack": one run over
//                                         a stack's PRs, bottom first
//   rerunAgentReview(review, targets)     "Re-run on new commits"

import { HARNESS_INFO, HARNESS_KINDS } from "$lib/bindings";
import { onInboxSynced } from "$lib/inbox/inboxStore.svelte";
import { acceptedEntry } from "$lib/review/findingText";
import { inboxPrTarget } from "$lib/review/prTarget";
import { startPrThread } from "$lib/review/prThreads";
import { readThread } from "$lib/services/api";
import { activeTurns, setThreadHandler } from "$lib/services/codexEvents.svelte";
import { availableHomes, commands } from "$lib/services/homeRouting";
import { isTauri } from "$lib/services/tauri";
import { type TurnEnd, turnEnd } from "$lib/services/turnLifecycle";
import { toastError } from "$lib/toaster";
import type {
  AgentReview,
  AgentReviewRequest,
  CheckoutTarget,
  CodexEvent,
  FocusArea,
  HarnessKind,
  InboxPr,
  PendingReview,
  ThreadDetail,
} from "$lib/types";
import { can } from "$lib/utils/capabilities";
import { errorMessage } from "$lib/utils/errors";

/** Each repository's agent reviews, newest first, as the desk last listed them. */
export const agentReviews = $state<{ byRepo: Record<string, AgentReview[]> }>({ byRepo: {} });

/** What a review looks at when nobody picked: Start review's defaults. */
export const DEFAULT_FOCUS: FocusArea[] = ["correctness", "tests"];

/** Runs whose thread is working, by thread id. */
const running = new Map<string, AgentReview>();
/** Threads whose turn ended before the queue knew them (a very fast turn). */
const endedEarly = new Map<string, TurnEnd["outcome"]>();
let listening = false;
let pumping: Promise<void> | null = null;

/** The harnesses this Home can run, Codex first: what Start review offers. */
export function installedHarnesses(): HarnessKind[] {
  const homes = new Set(availableHomes().map((entry) => entry.home.harness));
  const kinds = homes.size > 0 ? HARNESS_KINDS.filter((kind) => homes.has(kind)) : HARNESS_KINDS;
  return kinds.filter((kind) => homes.size > 0 || !HARNESS_INFO[kind].optional);
}

/** The harness a review defaults to: the first installed one that can work
 *  read-only, since a review at No checkout needs that. */
export function defaultReviewHarness(installed = installedHarnesses()): HarnessKind {
  return installed.find((kind) => can(kind, "readOnlySandbox")) ?? installed[0] ?? "codex";
}

/** A diff review of `targets` with the defaults: no checkout, drafts only. */
export function defaultRequest(targets: CheckoutTarget[], harness = defaultReviewHarness()): AgentReviewRequest {
  return {
    targets,
    harness,
    step: "noCheckout",
    focus: [...DEFAULT_FOCUS],
    keepDrafts: true,
    rerunOnNewCommits: false,
  };
}

export async function refreshAgentReviews(repoDir: string): Promise<AgentReview[]> {
  ensureQueue();
  const list = await commands.reviewAgentReviews(repoDir);
  agentReviews.byRepo[repoDir] = list;
  return list;
}

/** Queue a run; it starts as soon as the queue has room. */
export async function enqueueAgentReview(repoDir: string, request: AgentReviewRequest): Promise<AgentReview> {
  ensureQueue();
  const review = await commands.reviewEnqueueAgentReview(repoDir, request);
  await refreshAgentReviews(repoDir).catch(() => []);
  void pumpAgentReviews();
  return review;
}

/** "Review whole stack": one run over the stack's PRs (bottom first),
 *  filed under the top one. */
export async function startStackReview(
  repoDir: string,
  prs: InboxPr[],
  options: { viewer?: string | null; harness?: HarnessKind } = {},
): Promise<AgentReview> {
  const targets = prs.map((pr) => inboxPrTarget(repoDir, pr, options.viewer));
  return enqueueAgentReview(repoDir, defaultRequest(targets, options.harness));
}

/** Re-run `review` on `targets` (its PRs at their heads now): only the
 *  commits since its head when the head moved. */
export async function rerunAgentReview(review: AgentReview, targets: CheckoutTarget[]): Promise<AgentReview> {
  ensureQueue();
  const next = await commands.reviewRerunAgentReview(review.repoDir, review.id, targets);
  await refreshAgentReviews(review.repoDir).catch(() => []);
  void pumpAgentReviews();
  return next;
}

/** After an inbox sync: queue the re-runs that "Re-run when new commits
 *  land" asked for. */
export async function rerunOnNewCommits(repoDir: string, heads: CheckoutTarget[]): Promise<void> {
  if (heads.length === 0) return;
  const queued = await commands.reviewRerunOnNewCommits(repoDir, heads).catch(() => []);
  if (queued.length === 0) return;
  await refreshAgentReviews(repoDir).catch(() => []);
  void pumpAgentReviews();
}

onInboxSynced((repoDir, inbox) => {
  void rerunOnNewCommits(
    repoDir,
    inbox.prs.map((pr) => inboxPrTarget(repoDir, pr, inbox.viewer)),
  );
});

/** Start every run the desk lets start now. Calls run one after another. */
export function pumpAgentReviews(): Promise<void> {
  pumping ??= (async () => {
    try {
      for (;;) {
        const claimed = await commands.reviewClaimAgentReviews();
        if (claimed.length === 0) break;
        await Promise.all(claimed.map(startRun));
      }
    } catch (cause) {
      toastError(`Agent reviews: ${errorMessage(cause)}`);
    } finally {
      pumping = null;
    }
  })();
  return pumping;
}

async function startRun(review: AgentReview): Promise<void> {
  const target = review.request.targets.at(-1);
  try {
    if (!target) throw new Error("The agent review has no pull request");
    const prompt = await commands.reviewAgentReviewPrompt(review.repoDir, review.id);
    const { thread } = await startPrThread({
      repoDir: review.repoDir,
      target,
      input: [{ type: "text", text: prompt }],
      harness: review.request.harness,
    });
    const started = { ...review, threadId: thread.id };
    running.set(thread.id, started);
    await commands.reviewAgentReviewStarted(review.repoDir, review.id, thread.id);
    void refreshAgentReviews(review.repoDir).catch(() => []);
    const early = endedEarly.get(thread.id);
    if (early) {
      endedEarly.delete(thread.id);
      running.delete(thread.id);
      await collect(started, thread.id, early);
    }
  } catch (cause) {
    await finish(review, null, errorMessage(cause));
  }
}

/** The text of the thread's last agent message, or `null`. */
export function finalMessage(detail: ThreadDetail): string | null {
  for (const turn of [...detail.turns].reverse()) {
    const message = [...(turn.items ?? [])].reverse().find((item) => item.type === "agentMessage" && item.text);
    if (message?.text) return message.text;
  }
  return null;
}

const FAILED = "The agent's turn failed. Open the thread to see why.";
const INTERRUPTED = "The agent's turn was stopped before it finished.";

async function collect(review: AgentReview, threadId: string, outcome: TurnEnd["outcome"]): Promise<void> {
  if (outcome === "failed" || outcome === "interrupted") {
    await finish(review, null, outcome === "failed" ? FAILED : INTERRUPTED);
    return;
  }
  try {
    const detail = await readThread(threadId);
    // A run picked up after a restart has no turn event to go by.
    const status = detail.turns.at(-1)?.status;
    if (status === "failed" || status === "interrupted") {
      await finish(review, null, status === "failed" ? FAILED : INTERRUPTED);
      return;
    }
    const text = finalMessage(detail);
    await finish(review, text, text === null ? "The agent ended without a message" : null);
  } catch (cause) {
    await finish(review, null, errorMessage(cause));
  }
}

async function finish(review: AgentReview, message: string | null, error: string | null): Promise<void> {
  try {
    const done = await commands.reviewRecordAgentReviewResult(review.repoDir, review.id, message, error);
    if (done.status === "done" && done.findings > 0 && !review.request.keepDrafts) await addToPendingReview(done);
  } catch (cause) {
    toastError(`Agent review on #${review.number}: ${errorMessage(cause)}`);
  }
  await refreshAgentReviews(review.repoDir).catch(() => []);
  void pumpAgentReviews();
}

/** "Keep findings as drafts" off: the run's Findings go straight into their
 *  PR's pending review (still unposted until Submit), each PR of a stack
 *  review its own. */
async function addToPendingReview(review: AgentReview): Promise<void> {
  for (const target of review.request.targets) {
    const findings = (await commands.reviewFindings(review.repoDir, target.number)).filter(
      (finding) => finding.agentReviewId === review.id && finding.state === "draft",
    );
    if (findings.length === 0) continue;
    const stored = await commands.reviewPendingReview(review.repoDir, target.number);
    const pending: PendingReview = stored ?? {
      headSha: target.number === review.number ? review.headSha : target.headSha,
      entries: [],
      summary: "",
      verdict: "comment",
      updatedAt: 0,
    };
    const entries = findings.map((finding) => acceptedEntry(finding, `acceptedFinding-${finding.id}`));
    await commands.reviewSavePendingReview(review.repoDir, target.number, {
      ...pending,
      entries: [...pending.entries, ...entries],
    });
    for (const finding of findings) {
      await commands.reviewSetFindingState(review.repoDir, finding.id, "added", null, null);
    }
  }
}

function onEvent(event: CodexEvent) {
  const end = turnEnd(event);
  if (!end) return;
  const review = running.get(end.threadId);
  if (!review) {
    // Possibly a run whose thread id the queue hasn't recorded yet.
    if (pumping) {
      endedEarly.set(end.threadId, end.outcome);
      if (endedEarly.size > 20) endedEarly.delete(endedEarly.keys().next().value as string);
    }
    return;
  }
  running.delete(end.threadId);
  void collect(review, end.threadId, end.outcome);
}

/** Listen for turn ends once, and pick up runs a restart left behind. */
function ensureQueue() {
  if (listening) return;
  listening = true;
  setThreadHandler(onEvent);
  void resume();
}

async function resume(): Promise<void> {
  // The browser preview's seeded runs have no threads to read back, and
  // stay as seeded until something new is queued.
  if (!isTauri()) return;
  try {
    const active = await commands.reviewActiveAgentReviews();
    for (const review of active) {
      if (review.status !== "running") continue;
      if (!review.threadId) {
        await finish(review, null, "Pingex closed before the review started");
      } else if (activeTurns.list.includes(review.threadId)) {
        running.set(review.threadId, review);
      } else {
        await collect(review, review.threadId, "completed");
      }
    }
  } catch {
    // Nothing to resume, or the desk is unreachable: the next start retries.
  }
  void pumpAgentReviews();
}
