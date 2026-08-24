<script lang="ts">
import { ChevronRight, Pause, Play, Target, X } from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { nameNewThread } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import Composer from "$lib/composer/Composer.svelte";
import type { SlashCommandId } from "$lib/composer/slashCommands";
import RightPanel, { type PanelView } from "$lib/panels/RightPanel.svelte";
import { runsFor, setAgentRuns } from "$lib/services/agentRuns.svelte";
import {
  clearThreadGoal,
  compactThread,
  copyText,
  getThreadGoal,
  gitChangesSummary,
  gitRecentCommits,
  gitRepoInfo,
  gitWorktreeAdd,
  interruptTurn,
  invalidateThreadCache,
  isQueueUnsupported,
  isRevertUnsupported,
  killAgentRun,
  listAgentRuns,
  listSubagents,
  openInZed,
  queueAdd,
  queueDelete,
  queueList,
  queueReorder,
  queueUpdate,
  readThread,
  revealInFinder,
  revertThread,
  reviewLocalDiff,
  rollbackThread,
  setThreadGoal,
  setThreadGoalStatus,
  startReview,
  startThread,
  startTurn,
  updateSubagentPolicy,
} from "$lib/services/api";
import {
  activeTurns,
  approvals,
  type CodexEvent,
  elicitations,
  setThreadHandler,
  threadTokenUsage,
  turnPlans,
  userInputRequests,
} from "$lib/services/codexEvents.svelte";
import { processes } from "$lib/services/processes.svelte";
import ApprovalCard from "$lib/thread/ApprovalCard.svelte";
import { requestAutoName } from "$lib/thread/autoName";
import { contextStats as deriveContextStats } from "$lib/thread/contextUsage";
import DiscardQueuedDialog from "$lib/thread/DiscardQueuedDialog.svelte";
import ElicitationCard from "$lib/thread/ElicitationCard.svelte";
import FloatingMenu from "$lib/thread/FloatingMenu.svelte";
import { collectFileChanges } from "$lib/thread/fileChanges";
import { cwdBelongsTo } from "$lib/thread/handoff";
import { adoptLive, releaseLive, trackLive } from "$lib/thread/liveThreads.svelte";
import { messageText, messageTitle } from "$lib/thread/messageText";
import { planText } from "$lib/thread/planText";
import QuestionCard from "$lib/thread/QuestionCard.svelte";
import QueuedMessageRow from "$lib/thread/QueuedMessageRow.svelte";
import { isClientQueued, isLocalOnly, localId, mergeQueue, pendingId } from "$lib/thread/queueEntries";
import ReasoningBlock from "$lib/thread/ReasoningBlock.svelte";
import RewindThreadDialog from "$lib/thread/RewindThreadDialog.svelte";
import TurnPlanCard from "$lib/thread/TurnPlanCard.svelte";
import {
  applyThreadEvent,
  BUFFERING_NOTICE,
  ensureTurn,
  finalizeRunningTurns,
  upsertItem,
} from "$lib/thread/threadStream";
import {
  completedSegmentKey,
  segmentKey,
  splitTurn,
  turnDiffCount,
  turnSegments,
  workedLabel,
} from "$lib/thread/turnSegments";
import UserMessageBubble from "$lib/thread/UserMessageBubble.svelte";
import { estimateCost } from "$lib/thread/usageCost";
import WorkItem from "$lib/thread/WorkItem.svelte";
import { toastError } from "$lib/toaster";
import type {
  BootstrapData,
  ChangesSummary,
  FileUpdateChange,
  GitCommit,
  GitRepoInfo,
  QueuedSubmission,
  ReviewTarget,
  SideQuestion,
  SubagentDetail,
  SubagentPolicy,
  ThreadDetail,
  ThreadGoal,
  ThreadItem,
  ThreadTokenUsage,
  Turn,
  TurnOptions,
  UserInputPart,
  WorktreeBranchRequest,
} from "$lib/types";
import CreateWorktreeDialog from "$lib/worktrees/CreateWorktreeDialog.svelte";
import { tempWorktreeLocation } from "$lib/worktrees/worktrees";

let {
  threadId,
  cwd,
  projectPath = "",
  workspaceId = null,
  codexHome = null,
  expectedCwd = null,
  sideQuestions = [],
  onThreadCreated,
  onDataChanged,
  onCommand,
  onSelectThread,
  onOpenSubagent,
}: {
  threadId: string | null;
  cwd: string;
  /** Owning project's path; keys the composer's per-project draft. */
  projectPath?: string;
  /** A virtual workspace makes the backend derive a writable hub and roots. */
  workspaceId?: string | null;
  /** Active Codex home, used for restart-safe managed worktree locations. */
  codexHome?: string | null;
  /** Worktree a deep link asked for; a banner warns if the thread's cwd differs. */
  expectedCwd?: string | null;
  sideQuestions?: SideQuestion[];
  onThreadCreated?: (id: string, cwd: string) => void;
  onDataChanged?: (data: BootstrapData) => void;
  /** Thread-level slash commands from the composer (new, fork, archive, rename). */
  onCommand?: (command: SlashCommandId, threadId: string | null) => void;
  onSelectThread?: (id: string) => void;
  /** Open a subagent thread; the full detail lets the app navigate to
   *  subagents that bootstrap hasn't picked up yet (e.g. mid plan mode). */
  onOpenSubagent?: (agent: SubagentDetail) => void;
} = $props();

let thread = $state<ThreadDetail | null>(null);
/** What the thread last ran on, so a turn sent before the model list loads
 *  still carries full collaboration-mode settings. */
const lastTurnModel = $derived.by(() => {
  const turns = thread?.turns ?? [];
  for (let i = turns.length - 1; i >= 0; i--) {
    const model = turns[i].model;
    if (model) return model;
  }
  return null;
});
let loading = $state(false);
let error = $state<string | null>(null);
let streamError = $state<string | null>(null);
// Surface stream errors as dismissable, auto-expiring toasts (see ToastHost).
$effect(() => {
  if (!streamError) return;
  toastError(streamError);
  streamError = null;
});
/**
 * The last advisory Codex sent — a warning, a model reroute, a retryable error.
 * Deliberately separate from `streamError`: none of these ended the turn, and
 * showing them in the error card would make a thread that recovered look
 * broken. Cleared when the next turn starts.
 */
let notice = $state<string | null>(null);
/**
 * The goal this thread is working towards, or null. Seeded when the thread
 * opens and kept live from `thread/goal/updated`, so the banner reflects what
 * Codex holds — including a goal set from another client or an earlier session
 * — rather than only what was typed here.
 */
let goal = $state<ThreadGoal | null>(null);
let liveThreadId = $state<string | null>(null);
/** False once this view is torn down, so a late read cannot claim the thread. */
let attached = true;
let starting = $state(false);
let scroller: HTMLElement | null = null;
let panelView = $state<PanelView | null>(null);
let codexSubagents = $state<SubagentDetail[]>([]);
let subagentModelPolicy = $state<SubagentPolicy | null>(null);
let subagentReasoningEffortPolicy = $state<SubagentPolicy | null>(null);
/** Mirror of the server-side queue (`thread/queue/*`) for this thread. */
let queued = $state<QueuedSubmission[]>([]);
/** Per-message turn options — the server queue has no field for them, so they
 *  live only in this session, keyed by `clientUserMessageId`. */
let queuedOptions = new Map<string, TurnOptions>();
/** Local queue mutations in flight; while > 0, `thread/queue/changed` re-lists
 *  are skipped so they cannot clobber an optimistic entry. */
let queueMutations = 0;
let tokenUsage = $state<ThreadTokenUsage | null>(null);
let compacting = $state(false);
let composer = $state<{
  implementPlan: () => void;
  implementPlanFresh: (plan?: string | null) => void;
  appSubagentsChoice: () => boolean | null;
  openReviewPicker: () => void;
  restoreText: (text: string) => void;
  isEmpty: () => boolean;
} | null>(null);
/** Model the composer will run turns on — priced for the usage estimate. */
let activeModel = $state<string | null>(null);
type NewThreadLocation = "project" | "temporary" | "permanent";
let newThreadLocation = $state<NewThreadLocation>("project");
let newThreadCwd = $state("");
let repoInfo = $state<GitRepoInfo | null>(null);
let commits = $state<GitCommit[]>([]);

$effect(() => {
  if (threadId) return;
  const dir = projectPath || cwd;
  Promise.all([gitRepoInfo(dir), gitRecentCommits(dir, 20).catch(() => [])])
    .then(([info, recent]) => {
      repoInfo = info;
      commits = recent;
    })
    .catch(() => {
      repoInfo = null;
      commits = [];
    });
});

function chooseNewThreadLocation(location: NewThreadLocation) {
  if (location === "permanent") {
    openDialog(CreateWorktreeDialog, {
      codexHome,
      repoDir: repoInfo?.root ?? (projectPath || cwd),
      commits,
      submit: async (path: string, branch: WorktreeBranchRequest) => {
        await gitWorktreeAdd(repoInfo?.root ?? (projectPath || cwd), path, branch);
        newThreadLocation = "permanent";
        newThreadCwd = path;
      },
    });
    return;
  }
  newThreadLocation = location;
  newThreadCwd = cwd;
}

async function ensureNewThreadCwd(): Promise<string> {
  if (newThreadLocation === "project") return cwd;
  if (newThreadLocation === "permanent") return newThreadCwd;
  if (!codexHome) throw new Error("The active Codex home is unavailable.");
  const repoDir = repoInfo?.root ?? (projectPath || cwd);
  const suffix = `${Date.now().toString(36)}-${crypto.randomUUID().slice(0, 6)}`;
  const branchName = `codex/tmp-${suffix}`;
  const path = tempWorktreeLocation(codexHome, repoDir, suffix);
  await gitWorktreeAdd(repoDir, path, { kind: "new", name: branchName, base: null });
  newThreadCwd = path;
  return path;
}

// Deliberately two independent requests rather than one `Promise.all`:
// `listSubagents` resumes and re-reads every descendant thread, so it is slow
// and can stall. Gating our own runs behind it would leave the transcript's
// agent cards inert and the menu empty for as long as Codex takes — or forever.
async function refreshSubagents(id: string) {
  void (async () => {
    try {
      setAgentRuns(id, await listAgentRuns(id));
    } catch {
      // Leave whatever the store already had rather than blanking the menu.
    }
  })();
  try {
    const codex = await listSubagents(id);
    if (id === liveThreadId) codexSubagents = codex;
  } catch {
    if (id === liveThreadId) codexSubagents = [];
  }
}

/**
 * The two kinds of agent a thread can have, as one list.
 *
 * The app's half is derived rather than snapshotted: agents are spawned
 * mid-turn, and nothing re-runs `refreshSubagents` at that point — Codex's
 * `collabToolCall` event does not fire for our tools. Reading the store keeps
 * the menu live while they work.
 */
const appSubagentDetails = $derived(
  runsFor(liveThreadId)
    .filter((run) => run.childThreadId)
    .map((run) => ({
      id: run.childThreadId as string,
      parentThreadId: liveThreadId ?? "",
      title: run.name,
      cwd: run.cwd,
      status: run.status,
      agentNickname: run.name,
      agentRole: null,
      model: run.model,
      reasoningEffort: run.reasoningEffort,
      source: "app" as const,
      runId: run.runId,
    })),
);

const subagentDetails = $derived([
  ...codexSubagents.map((detail) => ({ ...detail, source: "codex" as const })),
  ...appSubagentDetails,
]);

$effect(() => {
  const id = threadId;
  if (!id) {
    if (!liveThreadId) {
      thread = { id: "", preview: "", cwd, turns: [] };
      loading = false;
      error = null;
    }
    return;
  }
  if (id === liveThreadId) return;
  liveThreadId = id;
  // Codex replays `thread/tokenUsage/updated` on a thread's first resume; after
  // that the cached figure is all we have until the next turn reports one.
  tokenUsage = threadTokenUsage[id] ?? null;
  goal = null;
  loadGoal(id);
  // A thread left mid-work keeps streaming into a retained document; adopt that
  // instead of re-reading a transcript that stops where the turn began.
  const held = adoptLive(id);
  if (held) {
    thread = held.detail;
    queued = held.queued;
    queuedOptions = held.queuedOptions;
    compacting = held.compacting;
    streamError = held.streamError;
    subagentModelPolicy = held.subagentModelPolicy;
    subagentReasoningEffortPolicy = held.subagentReasoningEffortPolicy;
    loading = false;
    error = null;
    refreshSubagents(id);
    return;
  }
  queued = [];
  queuedOptions = new Map();
  compacting = false;
  thread = null;
  loading = true;
  error = null;
  // The cached detail is keyed by the summary's `updated_at`, which does not
  // move while a turn runs — for a working thread it is stale by construction.
  const cacheReady = activeTurns.list.includes(id) ? invalidateThreadCache(id).catch(() => {}) : Promise.resolve();
  cacheReady
    .then(() => readThread(id))
    .then((detail) => {
      if (!attached || id !== liveThreadId) return;
      // A turn left `inProgress` by a session that has since died would render
      // as working forever — nothing can complete it, so show it as what it is.
      if (!activeTurns.list.includes(id)) {
        finalizeRunningTurns(detail.turns, "interrupted");
      }
      thread = trackLive(id, detail).detail;
      subagentModelPolicy = detail.subagentModelPolicy ?? null;
      subagentReasoningEffortPolicy = detail.subagentReasoningEffortPolicy ?? null;
      refreshSubagents(id);
      // Messages queued by an earlier session (or another client) are durable
      // on the server; pick them up so the drain effect can run them.
      refreshQueue(id);
    })
    .catch((cause) => {
      if (id !== liveThreadId) return;
      error = cause instanceof Error ? cause.message : String(cause);
    })
    .finally(() => {
      if (id === liveThreadId) loading = false;
    });
});

$effect(() => setThreadHandler(handleEvent));

// Hand the working state back on unmount: switching to another thread destroys
// this view, and a turn in flight has to keep streaming somewhere.
$effect(() => () => {
  attached = false;
  if (!liveThreadId) return;
  releaseLive(liveThreadId, {
    queued,
    queuedOptions,
    compacting,
    streamError,
    subagentModelPolicy,
    subagentReasoningEffortPolicy,
  });
});

function handleEvent(event: CodexEvent) {
  const { method, params } = event;
  if (method === "disconnected") {
    streamError = "Lost connection to Codex.";
    finalizeRunningTurns(thread?.turns ?? [], "interrupted");
    return;
  }
  if (method === "thread/status/changed" && params?.threadId) {
    const target = subagentDetails.find((candidate) => candidate.id === params.threadId);
    if (target) target.status = params.status?.type ?? params.status ?? target.status;
  }
  if (method === "thread/started" && liveThreadId && params?.thread?.parentThreadId) {
    refreshSubagents(liveThreadId);
  }
  if (method === "thread/goal/updated" && params?.threadId === liveThreadId) {
    goal = params.goal ?? null;
  }
  if (method === "thread/tokenUsage/updated" && params?.threadId === liveThreadId) {
    tokenUsage = params.tokenUsage ?? null;
  }
  if (method === "thread/compacted" && params?.threadId === liveThreadId) {
    compacting = false;
  }
  if (method === "thread/queue/changed" && params?.threadId === liveThreadId && liveThreadId) {
    refreshQueue(liveThreadId);
  }
  if (method === "thread/reverted" && params?.threadId === liveThreadId && liveThreadId && !starting) {
    // Another client truncated this thread's history; the local transcript and
    // cache are both stale. Force a re-read on next load.
    invalidateThreadCache(liveThreadId).catch(() => {});
  }
  if (method === "thread/settings/updated" && params?.threadId === liveThreadId) {
    subagentModelPolicy = params.threadSettings?.subagentModelPolicy ?? null;
    subagentReasoningEffortPolicy = params.threadSettings?.subagentReasoningEffortPolicy ?? null;
  }
  if (!thread || !params || params.threadId !== liveThreadId) return;
  if (method === "turn/started") notice = null;
  const outcome = applyThreadEvent(thread, event);
  if (outcome.streamError) streamError = outcome.streamError;
  if (outcome.notice) notice = outcome.notice;
  // The buffering notice describes an ongoing stall; once the stall ends — or
  // the turn does — it would just read as a hang, so take it down.
  if ((outcome.bufferingEnded || outcome.turnCompleted) && notice === BUFFERING_NOTICE) notice = null;
  // Compaction runs as a turn, so its end — however it ends — releases the meter.
  if (outcome.turnCompleted) compacting = false;
  if (outcome.turnCompleted && liveThreadId) invalidateThreadCache(liveThreadId).catch(() => {});
  // The end of the opening turn is the first moment a title can reflect what the
  // thread actually turned out to be about, so re-name off the exchange.
  if (outcome.turnCompleted && liveThreadId && thread.turns.length === 1) {
    requestAutoName(liveThreadId, "reply");
  }
  if (outcome.collabToolCall && liveThreadId) refreshSubagents(liveThreadId);
  maybeScroll();
}

function maybeScroll(force = false) {
  const element = scroller;
  if (!element) return;
  const nearBottom = element.scrollHeight - element.scrollTop - element.clientHeight < 120;
  if (force || nearBottom) {
    requestAnimationFrame(() => {
      element.scrollTop = element.scrollHeight;
    });
  }
}

/**
 * The thread id to run work on, creating the thread first when this view is
 * still an unsent draft. Callers own `starting` around it: it leaves the draft
 * adopted under its new id either way.
 */
async function ensureLiveThread(): Promise<string> {
  if (liveThreadId) return liveThreadId;
  if (!thread) throw new Error("This thread is not ready yet.");
  const newCwd = await ensureNewThreadCwd();
  const created = await startThread(newCwd, workspaceId, composer?.appSubagentsChoice() ?? null);
  liveThreadId = created.id;
  thread.id = created.id;
  // The draft is now a real thread running a turn: retain it under its id so
  // navigating away mid-turn keeps the stream.
  trackLive(created.id, thread);
  onThreadCreated?.(created.id, created.cwd ?? newCwd);
  return created.id;
}

/** Returns whether the message is accounted for — started as a turn or safely
 *  queued. `false` means it reached nothing, so a caller holding the only copy
 *  (see `drain`) must put it back. */
async function send(input: UserInputPart[], options?: TurnOptions): Promise<boolean> {
  if (!thread) return false;
  if (activeTurn || starting) {
    // Codex is mid-turn: park the message on the server-side queue and send it
    // once the turn ends (completed or interrupted via Stop/Esc).
    enqueue(input, options);
    return true;
  }
  const text = input
    .filter((part) => part.type === "text")
    .map((part) => part.text ?? "")
    .join("");
  streamError = null;
  // Sending again is the retry for a drain that failed, so let the queue move.
  drainBlocked = false;
  const localTurnId = `local-${Date.now()}`;
  try {
    const isFirstMessage = !liveThreadId;
    if (isFirstMessage) starting = true;
    const id = await ensureLiveThread();
    // Name the thread off its opening message before the turn is even started:
    // nothing else can title it until its rollout persists, so waiting here is
    // what leaves the sidebar reading "Untitled thread" for the whole turn.
    if (isFirstMessage) {
      nameNewThread(id, messageTitle(input));
      if (text.trim()) requestAutoName(id, "seed", text);
    }
    thread.turns.push({
      id: localTurnId,
      status: "inProgress",
      // What this turn runs on, so its replies are labelled as they stream in
      // rather than only after the thread is read back.
      model: options?.resolvedModel ?? null,
      reasoningEffort: options?.resolvedEffort ?? null,
      items: [
        {
          type: "userMessage",
          id: `local-item-${Date.now()}`,
          content: input,
        },
      ],
    });
    maybeScroll(true);
    const start = startTurn(id, input, options);
    pendingTurnStart = start;
    const turn = await start;
    // `turn/started` may already have renamed the turn to the id Codex actually
    // runs it under (which can differ from the one returned here); leave that.
    const pending = thread.turns.find((candidate) => candidate.id === localTurnId);
    if (pending) {
      pending.id = turn.id;
      pending.status = turn.status ?? "inProgress";
    }
    return true;
  } catch (cause) {
    thread.turns = thread.turns.filter((candidate) => candidate.id !== localTurnId);
    streamError = cause instanceof Error ? cause.message : String(cause);
    return false;
  } finally {
    starting = false;
    pendingTurnStart = null;
  }
}

/** Editing a past user message rewinds the thread to just before that turn and
 *  resends on the same thread — no fork, so the conversation keeps its id and
 *  its place in the sidebar. Destructive, hence the confirmation. */
async function submitEdit(turn: Turn, text: string) {
  if (!thread || !liveThreadId || turn.id.startsWith("local-")) return;
  const threadIdAtEdit = liveThreadId;
  const turnsToDrop = () => {
    const turns = thread?.turns;
    if (!turns) return null;
    const index = turns.findIndex((candidate) => candidate.id === turn.id);
    return index === -1 ? null : { index, count: turns.length - index };
  };
  const before = turnsToDrop();
  if (!before) return;
  if (!(await openDialog(RewindThreadDialog, { turnCount: before.count }))) return;
  // The view may have moved on, or the transcript grown, while the dialog was
  // open — re-resolve the turn against current state before truncating.
  if (!thread || liveThreadId !== threadIdAtEdit) return;
  const target = turnsToDrop();
  if (!target) return;
  streamError = null;
  starting = true;
  try {
    // `thread/revert` is the current truncation API; `thread/rollback` is
    // deprecated upstream but kept as the fallback for codex CLIs (≤0.146)
    // that predate revert. Any other revert failure is a real error.
    const keptTurnIds = thread.turns.slice(0, target.index).map((candidate) => candidate.id);
    try {
      await revertThread(threadIdAtEdit, turn.id, keptTurnIds);
    } catch (cause) {
      if (!isRevertUnsupported(cause)) throw cause;
      await rollbackThread(threadIdAtEdit, target.count);
    }
    if (!thread || liveThreadId !== threadIdAtEdit) return;
    thread.turns = thread.turns.slice(0, target.index);
  } catch (cause) {
    streamError = cause instanceof Error ? cause.message : String(cause);
    return;
  } finally {
    starting = false;
  }
  // `send` owns the optimistic bubble, queueing and turn options, and keeps
  // `liveThreadId` unchanged so the stream events still match this view.
  await send([{ type: "text", text }]);
}

const activeTurn = $derived(thread?.turns.find((candidate) => candidate.status === "inProgress") ?? null);
const threadApprovals = $derived(approvals.list.filter((approval) => approval.threadId === liveThreadId));
const threadQuestions = $derived(userInputRequests.list.filter((request) => request.threadId === liveThreadId));
const threadElicitations = $derived(elicitations.list.filter((entry) => entry.threadId === liveThreadId));
const turnPlan = $derived(liveThreadId ? (turnPlans.byThread[liveThreadId] ?? null) : null);
/**
 * A persisted question is answerable in place unless its request is still live
 * in this session — that one already has its own card below the transcript.
 */
function strandedContext(item: ThreadItem, turnId: string) {
  if (!item.unanswered || !liveThreadId) return undefined;
  if (threadQuestions.some((request) => request.itemId === item.id)) return undefined;
  return {
    threadId: liveThreadId,
    turnId,
    onResume: async (text: string) => {
      await send([{ type: "text", text }]);
    },
    onAnswered: (answered: ThreadItem) => thread && upsertItem(thread.turns, turnId, answered),
  };
}

// Dots stand in for every gap where Codex is thinking rather than emitting —
// including after a preamble ("I'm adding x to y") while the tool call is still
// being written. Only text actively streaming in makes them redundant.
const showTypingIndicator = $derived.by(() => {
  if (!activeTurn || threadApprovals.length > 0 || threadQuestions.length > 0) return false;
  if (threadElicitations.length > 0) return false;
  const last = activeTurn.items.at(-1);
  return !(last?.type === "agentMessage" && last.streaming);
});

/** Park a message on the server-side queue, rendering it optimistically.
 *
 *  A failure here never drops the message: the entry stays in `queued` as a
 *  local-only one and still drains when the turn finishes. The server queue is
 *  durable and visible to other clients where the local one is not, so the loss
 *  is persistence, not the message. */
async function enqueue(input: UserInputPart[], options?: TurnOptions) {
  const threadIdAtAdd = liveThreadId;
  const clientUserMessageId = crypto.randomUUID();
  if (options) queuedOptions.set(clientUserMessageId, options);
  // Draft thread: there is nothing to queue against yet, so it is local from
  // the start and drains once the thread exists.
  const id = threadIdAtAdd ? pendingId(clientUserMessageId) : localId(clientUserMessageId);
  const optimistic: QueuedSubmission = { id, input, clientUserMessageId };
  queued.push(optimistic);
  if (!threadIdAtAdd) return;
  queueMutations++;
  try {
    const submission = await queueAdd(threadIdAtAdd, input, clientUserMessageId);
    const index = queued.findIndex((entry) => entry.id === optimistic.id);
    if (index >= 0) queued[index] = submission;
  } catch (cause) {
    const index = queued.findIndex((entry) => entry.id === optimistic.id);
    if (index >= 0) queued[index] = { ...optimistic, id: localId(clientUserMessageId) };
    // An unsupported queue is a property of this Codex, not something that went
    // wrong — the chip says "Queued locally" and that is the whole story. Other
    // failures (a full queue, a lost thread) are worth a word, but as a notice:
    // nothing ended the turn, and the message is still going to send.
    if (!isQueueUnsupported(cause)) {
      const message = cause instanceof Error ? cause.message : String(cause);
      notice = `Queued in this window only — Codex could not hold it (${message}). It will send when this turn finishes.`;
    }
  } finally {
    queueMutations--;
  }
}

/** Remove a queued message, server-side first so the chip cannot resurrect. */
async function removeQueued(entry: QueuedSubmission) {
  queued = queued.filter((candidate) => candidate.id !== entry.id);
  queuedOptions.delete(entry.clientUserMessageId);
  if (!liveThreadId || isClientQueued(entry)) return;
  queueMutations++;
  try {
    await queueDelete(liveThreadId, entry.id);
  } catch {
    // Already gone (started or deleted elsewhere) — the re-list will settle it.
  } finally {
    queueMutations--;
  }
}

/** Re-mirror the server queue, unless our own mutation is still in flight. */
function refreshQueue(id: string) {
  if (queueMutations > 0) return;
  queueList(id)
    .then((items) => {
      if (id !== liveThreadId || queueMutations > 0) return;
      // Keeps the client-only entries the server does not know about — both the
      // ones still in flight and the ones it will never hold.
      queued = mergeQueue(items, queued);
    })
    .catch(() => {});
}

let draining = $state(false);
/** Set when a drain put its message back, because nothing else in the effect's
 *  guard would have changed — without this the retry fires again immediately
 *  and spins. Cleared when the user next sends, which is also the retry. */
let drainBlocked = $state(false);
$effect(() => {
  if (draining || drainBlocked || activeTurn || starting || loading || queued.length === 0) return;
  const next = queued[0];
  if (next) drain(next);
});

/** Run the head of the queue: take it off the server, then send it through the
 *  normal turn path so its options (which the server queue cannot hold) and the
 *  optimistic bubble behave exactly like a direct send.
 *
 *  Between the removal and the send this holds the only copy of the message, so
 *  a failed send puts it back rather than dropping it. */
async function drain(next: QueuedSubmission) {
  draining = true;
  const options = queuedOptions.get(next.clientUserMessageId);
  try {
    await removeQueued(next);
    queuedOptions.delete(next.clientUserMessageId);
    if (await send(next.input, options)) return;
    if (options) queuedOptions.set(next.clientUserMessageId, options);
    queued = [{ ...next, id: localId(next.clientUserMessageId) }, ...queued];
    drainBlocked = true;
  } finally {
    draining = false;
  }
}

/** Jump a queued message to the head and stop the running turn so it goes next. */
async function sendNow(entry: QueuedSubmission) {
  queued = [entry, ...queued.filter((candidate) => candidate.id !== entry.id)];
  if (liveThreadId && !queued.some(isClientQueued) && queued.length > 1) {
    queueMutations++;
    try {
      await queueReorder(
        liveThreadId,
        queued.map((candidate) => candidate.id),
      );
    } catch {
      // The local order still drives the drain; the server's is only cosmetic.
    } finally {
      queueMutations--;
    }
  }
  drainBlocked = false;
  await interrupt();
}

/** Replace a queued message's content, on the server too when it lives there. */
async function editQueued(entry: QueuedSubmission, input: UserInputPart[]) {
  queued = queued.map((candidate) => (candidate.id === entry.id ? { ...candidate, input } : candidate));
  if (!liveThreadId || isClientQueued(entry)) return;
  queueMutations++;
  try {
    await queueUpdate(liveThreadId, entry.id, input);
  } catch (cause) {
    toastError(`Could not update the queued message: ${cause instanceof Error ? cause.message : String(cause)}`);
  } finally {
    queueMutations--;
  }
}

/** Take a message off the queue: back into an empty composer, or, when that
 *  would clobber typed text, only after the user agrees to lose it. */
async function cancelQueued(entry: QueuedSubmission) {
  const text = queuedPreview(entry.input);
  if (composer?.isEmpty()) {
    await removeQueued(entry);
    composer.restoreText(text);
    return;
  }
  if (!(await openDialog(DiscardQueuedDialog, { preview: text }))) return;
  await removeQueued(entry);
}

function queuedPreview(input: UserInputPart[]) {
  return input
    .map((part) => (part.type === "text" ? (part.text ?? "") : `@${part.name}`))
    .join("")
    .trim();
}

/** The in-flight `startTurn` call, so a Stop pressed before it resolves can
 *  wait for the real turn id instead of silently doing nothing. */
let pendingTurnStart: Promise<unknown> | null = null;

async function interrupt() {
  if (!liveThreadId || !activeTurn) return;
  if (activeTurn.id.startsWith("local-")) {
    // The optimistic turn is still waiting on `turn/start`; Codex has never
    // heard of it. Wait for the real id, then interrupt that.
    try {
      await pendingTurnStart;
    } catch {
      return; // send() already surfaced the error and removed the turn.
    }
    if (!liveThreadId || !activeTurn || activeTurn.id.startsWith("local-")) return;
  }
  interruptTurn(liveThreadId, activeTurn.id).catch((cause) => {
    streamError = cause instanceof Error ? cause.message : String(cause);
  });
}

const allItems = $derived((thread?.turns ?? []).flatMap((turn) => turn.items));
/** Prior prompts, oldest first, for the composer's ↑/↓ recall. */
const messageHistory = $derived.by(() => {
  const out: string[] = [];
  for (const item of allItems) {
    if (item.type !== "userMessage" || !item.content) continue;
    const text = messageText(item.content as UserInputPart[]).trim();
    if (text && out.at(-1) !== text) out.push(text);
  }
  return out;
});
const latestPlan = $derived.by(() => {
  for (let i = allItems.length - 1; i >= 0; i--) {
    const text = planText(allItems[i]);
    if (text) return text;
  }
  return null;
});
// A plan the user hasn't responded to yet — any later user message (e.g.
// "Implement the plan.") means the plan actions should stay hidden.
const pendingPlan = $derived.by(() => {
  for (let i = allItems.length - 1; i >= 0; i--) {
    const item = allItems[i];
    if (item.type === "userMessage") return null;
    const text = planText(item);
    if (text) return text;
  }
  return null;
});
// Latest diff per changed file across the whole thread, in first-touched order.
const outputChanges = $derived(collectFileChanges(allItems));
/**
 * Working-tree diff requested by `/diff`. Non-null only while that view is up:
 * it answers "what is uncommitted right now", which is a different question
 * from `outputChanges` ("what did this thread touch").
 */
let workingDiff = $state<FileUpdateChange[] | null>(null);
const panelChanges = $derived(workingDiff ?? outputChanges);

/**
 * Git-derived "Changes" summary for the thread's directory. Only the cheap
 * summary is fetched here (numstat, no diff bodies); it refreshes when a turn
 * finishes, never on a timer, and each file's patch loads on demand.
 */
let gitChanges = $state<ChangesSummary | null>(null);
let gitChangesLoading = $state(false);
let gitChangesError = $state<string | null>(null);
let gitChangesTimer: ReturnType<typeof setTimeout> | null = null;
let gitChangesRequest = 0;
const changesDir = $derived(workspaceId ? cwd : thread?.cwd || cwd);

function refreshGitChanges(immediate = false) {
  if (gitChangesTimer) clearTimeout(gitChangesTimer);
  gitChangesTimer = setTimeout(
    async () => {
      gitChangesTimer = null;
      const dir = changesDir;
      if (!dir) return;
      const id = ++gitChangesRequest;
      gitChangesLoading = true;
      try {
        const summary = await gitChangesSummary(dir);
        if (id !== gitChangesRequest) return;
        gitChanges = summary;
        gitChangesError = null;
      } catch (cause) {
        if (id !== gitChangesRequest) return;
        gitChanges = null;
        gitChangesError = cause instanceof Error ? cause.message : String(cause);
      } finally {
        if (id === gitChangesRequest) gitChangesLoading = false;
      }
    },
    immediate ? 0 : 500,
  );
}

// Load once per directory, then again each time a turn ends.
$effect(() => {
  changesDir;
  gitChanges = null;
  refreshGitChanges(true);
});
let hadActiveTurn = false;
$effect(() => {
  const running = activeTurn !== null;
  if (hadActiveTurn && !running) refreshGitChanges();
  hadActiveTurn = running;
});
const sourceQueries = $derived(
  allItems.filter((item) => item.type === "webSearch" && item.query).map((item) => item.query as string),
);

function implementPlan() {
  panelView = null;
  // Route through the composer so plan mode is switched off and the turn
  // carries the composer's model/permission options.
  composer?.implementPlan();
}

function implementPlanFresh() {
  // The panel can show an older plan than the composer's pending one, so hand
  // over the plan the user is actually looking at.
  const shown = panelView?.kind === "plan" ? panelView.text : latestPlan;
  panelView = null;
  composer?.implementPlanFresh(shown);
}

/**
 * "Clear context & implement": run the plan in a brand-new thread on the same
 * directory, seeded with the plan itself, then move the view onto it. The
 * planning conversation stays on disk untouched — the implementation just
 * starts from an empty context window.
 */
async function startPlanThread(input: UserInputPart[], options?: TurnOptions) {
  if (starting || activeTurn) return;
  starting = true;
  streamError = null;
  const dir = workspaceId ? cwd : thread?.cwd || cwd;
  try {
    const created = await startThread(dir, workspaceId, composer?.appSubagentsChoice() ?? null);
    await startTurn(created.id, input, options);
    // Adopting the new thread re-drives the load effect, so the view swaps to it.
    onThreadCreated?.(created.id, created.cwd ?? dir);
    // The plan itself is this thread's opening message, so it names it.
    nameNewThread(created.id, messageTitle(input));
    const plan = messageText(input);
    if (plan.trim()) requestAutoName(created.id, "seed", plan);
  } catch (cause) {
    streamError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    starting = false;
  }
}

const contextStats = $derived(tokenUsage ? deriveContextStats(tokenUsage) : null);

// A deep link asked for a specific worktree but the opened thread runs
// elsewhere — surface it rather than silently showing the wrong directory.
const cwdMismatch = $derived(
  Boolean(expectedCwd && thread?.id && thread?.cwd && !cwdBelongsTo(thread.cwd, expectedCwd)),
);

/** `/compact` is answered here rather than in App because it needs the live thread. */
async function compact() {
  if (!liveThreadId || compacting || activeTurn || starting) return;
  compacting = true;
  streamError = null;
  try {
    await compactThread(liveThreadId);
  } catch (cause) {
    compacting = false;
    streamError = cause instanceof Error ? cause.message : String(cause);
  }
}

/**
 * `/review` — hand the chosen target to Codex as a review turn. A review needs
 * no conversation behind it, so an unsent draft becomes a real thread here
 * rather than turning the command away.
 *
 * The turn is seeded from the response rather than left to the stream: a review
 * sends no `turn/started`, so without this the turn only exists once its first
 * item arrives, and Stop in the meantime would name the wrong turn.
 */
async function review(target: ReviewTarget) {
  if (activeTurn || starting) return;
  streamError = null;
  const fresh = !liveThreadId;
  if (fresh) starting = true;
  try {
    const turn = await startReview(await ensureLiveThread(), target);
    if (thread) {
      ensureTurn(thread.turns, turn.id).status = turn.status ?? "inProgress";
      maybeScroll(true);
    }
  } catch (cause) {
    streamError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (fresh) starting = false;
  }
}

/** `/diff` — show the working tree against HEAD in the Changes panel. */
async function showWorkingDiff() {
  const dir = thread?.cwd;
  if (!dir) return;
  try {
    const files = await reviewLocalDiff(dir, "HEAD");
    workingDiff = files.map((file) => ({
      path: file.path,
      kind: { type: diffKind(file.status), movePath: file.oldPath },
      diff: file.patch,
    }));
    panelView = { kind: "diffs" };
  } catch (cause) {
    streamError = cause instanceof Error ? cause.message : String(cause);
  }
}

/** Map a review file status onto the panel's change kinds. */
function diffKind(status: string): string {
  if (status === "added") return "add";
  if (status === "removed") return "delete";
  return "update";
}

/**
 * Thread-scoped commands. Those needing the live thread are answered here;
 * the rest bubble to App. A command that needs a thread and has none tells the
 * user so rather than silently doing nothing.
 */
function runCommand(command: SlashCommandId, argument = "", typed = "") {
  // These read back the conversation, so they cannot be answered on a draft.
  // `/review` and `/goal` are absent deliberately: both start a thread of their
  // own rather than turning the user away.
  if (!liveThreadId && (command === "compact" || command === "undo" || command === "copy" || command === "export")) {
    failCommand(command, typed);
    return;
  }
  if (command === "compact") {
    compact();
    return;
  }
  if (command === "review") {
    if (activeTurn || starting) {
      notice = "/review can't start while Codex is working — stop the current turn first.";
      return;
    }
    // A typed argument is the review instruction itself; with none, ask what to
    // review rather than assuming the working tree.
    if (argument) void review({ type: "custom", instructions: argument });
    else composer?.openReviewPicker();
    return;
  }
  if (command === "diff") {
    void showWorkingDiff();
    return;
  }
  if (command === "undo") {
    const turns = Number.parseInt(argument, 10);
    void undoTurns(Number.isFinite(turns) && turns > 0 ? turns : 1);
    return;
  }
  if (command === "status") {
    panelView = { kind: "status" };
    return;
  }
  if (command === "goal") {
    void goalCommand(argument, typed);
    return;
  }
  if (command === "copy") {
    void copyLastResponse();
    return;
  }
  if (command === "export") {
    void exportConversation();
    return;
  }
  if (command !== "new" && !liveThreadId) {
    failCommand(command, typed);
    return;
  }
  onCommand?.(command, liveThreadId);
}

/** A command that could not run: say why, and give the typed line back so the
 *  user can edit it rather than retyping it from scratch. */
function failCommand(command: SlashCommandId, typed: string, message?: string) {
  streamError = message ?? `/${command} needs an open thread.`;
  if (typed) composer?.restoreText(typed);
}

/**
 * `/goal` — set, view, or clear the goal for a long-running task. With no
 * argument the current goal is shown; `clear` drops it; anything else becomes
 * the new objective.
 *
 * Setting an objective needs no conversation behind it, so — like `/review` —
 * an unsent draft becomes a real thread here: that is what starting a thread
 * with a goal means. Reading and clearing stay draft-only answers, since
 * creating a thread just to report it has no goal would be waste.
 */
async function goalCommand(argument: string, typed = "") {
  const objective = argument && argument.toLowerCase() !== "clear" ? argument : null;
  const current = liveThreadId;
  if (!objective) {
    // Reading or clearing a goal a draft cannot have yet.
    if (!current) {
      notice = "No goal is set — /goal <objective> sets one.";
      return;
    }
    try {
      if (argument) {
        await clearThreadGoal(current);
        goal = null;
        notice = "Goal cleared.";
      } else {
        goal = await getThreadGoal(current);
        notice = goal ? `Goal (${goal.status}): ${goal.objective}` : "No goal is set — /goal <objective> sets one.";
      }
    } catch (cause) {
      failCommand("goal", typed, cause instanceof Error ? cause.message : String(cause));
    }
    return;
  }
  const fresh = !current;
  if (fresh && (activeTurn || starting)) {
    failCommand("goal", typed, "/goal can't start a thread while Codex is working.");
    return;
  }
  if (fresh) starting = true;
  try {
    const id = await ensureLiveThread();
    goal = await setThreadGoal(id, objective);
    notice = `Goal set: ${goal.objective}`;
    // A goal-only thread has no turn to name it from, so the objective titles
    // it: shown at once, then refined by the namer.
    if (fresh) {
      nameNewThread(id, messageTitle([{ type: "text", text: objective }]));
      requestAutoName(id, "seed", objective);
    }
  } catch (cause) {
    failCommand("goal", typed, cause instanceof Error ? cause.message : String(cause));
  } finally {
    if (fresh) starting = false;
  }
}

/** Read the goal Codex holds for a thread, if any; failures leave no goal shown. */
function loadGoal(id: string) {
  void getThreadGoal(id)
    .then((current) => {
      if (id === liveThreadId) goal = current;
    })
    .catch(() => {});
}

/** Pause a running goal, or resume a paused (or otherwise stalled) one. */
async function toggleGoal() {
  if (!goal || !liveThreadId) return;
  const status = goal.status === "active" ? "paused" : "active";
  try {
    goal = { ...goal, ...(await setThreadGoalStatus(liveThreadId, status)), objective: goal.objective };
  } catch (cause) {
    toastError(cause instanceof Error ? cause.message : String(cause));
  }
}

async function clearGoal() {
  if (!liveThreadId) return;
  try {
    await clearThreadGoal(liveThreadId);
    goal = null;
  } catch (cause) {
    toastError(cause instanceof Error ? cause.message : String(cause));
  }
}

const GOAL_STATUS_LABEL: Record<string, string> = {
  active: "active",
  paused: "paused",
  blocked: "blocked",
  usageLimited: "waiting on usage limit",
  budgetLimited: "budget exhausted",
  complete: "complete",
};

/** The markdown a transcript item contributes to `/copy` and `/export`. */
function itemMarkdown(item: ThreadItem): string {
  if (item.type === "agentMessage") return item.text ?? "";
  if (item.type !== "userMessage") return "";
  const parts = (item.content ?? []) as UserInputPart[];
  return parts
    .map((part) => (typeof part === "string" ? part : (part.text ?? "")))
    .filter(Boolean)
    .join("\n");
}

/** `/copy` — put the last completed response on the clipboard as markdown. */
async function copyLastResponse() {
  const items = thread?.turns.flatMap((turn) => turn.items) ?? [];
  const last = [...items].reverse().find((item) => item.type === "agentMessage" && !item.streaming && item.text);
  if (!last?.text) {
    notice = "No response to copy yet.";
    return;
  }
  try {
    await copyText(last.text);
    notice = "Copied the last response as markdown.";
  } catch (cause) {
    streamError = cause instanceof Error ? cause.message : String(cause);
  }
}

/** `/export` — put the whole conversation on the clipboard as markdown. */
async function exportConversation() {
  const sections: string[] = [];
  for (const turn of thread?.turns ?? []) {
    for (const item of turn.items) {
      const text = itemMarkdown(item);
      if (text) sections.push(`## ${item.type === "userMessage" ? "User" : "Codex"}\n\n${text}`);
    }
  }
  if (sections.length === 0) {
    notice = "Nothing to export yet.";
    return;
  }
  try {
    await copyText(sections.join("\n\n"));
    notice = "Copied the conversation as markdown.";
  } catch (cause) {
    streamError = cause instanceof Error ? cause.message : String(cause);
  }
}

/**
 * `/undo` — rewind the last N turns in place, keeping the thread id, so the
 * conversation rewinds rather than branching. Mirrors the edit-a-past-message
 * path above, minus the resend.
 */
async function undoTurns(turns: number) {
  if (!liveThreadId || activeTurn || starting || !thread) return;
  const threadIdAtUndo = liveThreadId;
  const dropped = Math.min(turns, thread.turns.length);
  if (dropped === 0) return;
  streamError = null;
  starting = true;
  try {
    await rollbackThread(threadIdAtUndo, dropped);
    if (!thread || liveThreadId !== threadIdAtUndo) return;
    thread.turns = thread.turns.slice(0, thread.turns.length - dropped);
  } catch (cause) {
    streamError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    starting = false;
  }
}

function changeSubagentPolicy(modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null) {
  subagentModelPolicy = modelPolicy;
  subagentReasoningEffortPolicy = effortPolicy;
  if (!liveThreadId) return;
  updateSubagentPolicy(liveThreadId, modelPolicy, effortPolicy).catch((cause) => {
    streamError = cause instanceof Error ? cause.message : String(cause);
  });
}
</script>

<div class="flex h-full">
<div class="relative flex h-full min-w-0 flex-1 flex-col">
  {#if thread && liveThreadId}
    <FloatingMenu
      plan={latestPlan}
      outputs={outputChanges}
      sources={sourceQueries}
      sideQuestionCount={sideQuestions.filter((entry) => entry.parentThreadId === liveThreadId).length}
      subagents={subagentDetails}
      processes={processes.list}
      currentThreadId={liveThreadId}
      {contextStats}
      costUsd={estimateCost(tokenUsage, activeModel)}
      onOpenFinder={() => revealInFinder(thread?.cwd || cwd).catch(() => {})}
      onOpenZed={() => openInZed(thread?.cwd || cwd).catch((cause) => (streamError = cause instanceof Error ? cause.message : String(cause)))}
      onShowPlan={() => (panelView = latestPlan ? { kind: "plan", text: latestPlan } : panelView)}
      onShowSources={() => (panelView = { kind: "sources", queries: sourceQueries })}
      onShowSideQuestions={() => (panelView = { kind: "side" })}
      onShowDiff={(path) => {
        workingDiff = null;
        panelView = { kind: "diffs", focusPath: path };
      }}
      gitChanges={gitChanges}
      onShowChanges={(path) => {
        refreshGitChanges();
        panelView = { kind: "changes", focusPath: path };
      }}
      onShowFiles={() => (panelView = { kind: "files" })}
      onShowMessageLog={() => (panelView = { kind: "messageLog" })}
      onOpenSubagent={(agent) => (onOpenSubagent ? onOpenSubagent(agent) : onSelectThread?.(agent.id))}
      onStopSubagent={async (agent) => {
        if (!agent.runId) return;
        await killAgentRun(agent.runId).catch(() => {});
        if (threadId) refreshSubagents(threadId);
      }}
      onOpenProcess={(process) => (panelView = { kind: "process", processKey: process.key })}
    />
  {/if}
  {#if cwdMismatch}
    <div class="mx-auto w-full max-w-3xl px-6 pt-4">
      <div class="card preset-tonal-warning px-3 py-2 text-xs leading-5">
        This thread's working directory (<span class="font-mono">{thread?.cwd}</span>) does not match the requested
        worktree (<span class="font-mono">{expectedCwd}</span>).
      </div>
    </div>
  {/if}
  <div class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto" bind:this={scroller}>
    <div class="mx-auto min-w-0 max-w-3xl space-y-5 px-6 py-8">
      {#if loading}
        <div class="space-y-3" aria-label="Loading thread">
          <div class="placeholder h-16 animate-pulse rounded-xl"></div>
          <div class="placeholder h-24 animate-pulse rounded-xl opacity-70"></div>
          <div class="placeholder h-16 animate-pulse rounded-xl opacity-40"></div>
        </div>
      {:else if error}
        <div class="card preset-tonal-error p-4 text-sm">
          <div class="font-semibold">Could not load this thread</div>
          <p class="mt-1 text-xs leading-5">{error}</p>
        </div>
      {:else if thread}
        {#each thread.turns as turn (turn.id)}
          {@const parts = splitTurn(turn)}
          {@const collapseDiffs = turnDiffCount(turn) > 1}
          {@const liveSegment = turn.status === "inProgress" ? parts.body.at(-1) : undefined}
          {#each parts.users as item (item.id)}
            <UserMessageBubble
              {item}
              cwd={thread?.cwd || cwd}
              editable={!!liveThreadId && !turn.id.startsWith("local-")}
              onSubmitEdit={(text) => submitEdit(turn, text)}
            />
          {/each}
          {@const firstWork = parts.body.find((segment) => segment.kind === "work")}
          {#each parts.body as segment (completedSegmentKey(segment))}
            {#if segment.kind === "message"}
              <WorkItem
                item={segment.item}
                stranded={strandedContext(segment.item, turn.id)}
                model={turn.model}
                effort={turn.reasoningEffort}
              />
            {:else if segment === liveSegment}
              {@const liveSegments = turnSegments(segment.items)}
              {#each liveSegments as liveSeg, liveIndex (segmentKey(liveSeg))}
                {#if liveSeg.kind === "reasoning"}
                  <ReasoningBlock
                    items={liveSeg.items}
                    live={liveIndex === liveSegments.length - 1}
                  />
                {:else}
                  <WorkItem item={liveSeg.item} {collapseDiffs} />
                {/if}
              {/each}
            {:else}
              <div>
                <Collapsible>
                  <Collapsible.Trigger class="group flex items-center gap-1 text-sm text-surface-500 hover:text-surface-700-300">
                    <span>{segment === firstWork && turn.status !== "inProgress" ? workedLabel(turn) : "Worked"}</span>
                    <ChevronRight size={14} class="transition group-data-[state=open]:rotate-90" />
                  </Collapsible.Trigger>
                  <Collapsible.Content>
                    <div class="mt-3 space-y-4 border-l-2 border-surface-200-800 pl-4">
                      {#each segment.items as item (item.id)}
                        <WorkItem {item} {collapseDiffs} />
                      {/each}
                    </div>
                  </Collapsible.Content>
                </Collapsible>
                <hr class="mt-3 border-surface-200-800" />
              </div>
            {/if}
          {/each}
          {#if turn.status === "failed" && turn.error}
            <div class="card preset-tonal-error p-3 text-xs">{turn.error.message}</div>
          {/if}
        {/each}

        {#each threadApprovals as approval (approval.requestId)}
          <ApprovalCard {approval} />
        {/each}

        {#each threadQuestions as request (request.requestId)}
          <QuestionCard {request} onAnswered={(item) => thread && upsertItem(thread.turns, request.turnId, item)} />
        {/each}

        {#each threadElicitations as elicitation (elicitation.requestId)}
          <ElicitationCard {elicitation} />
        {/each}

        {#if showTypingIndicator}
          <div class="flex items-center gap-1.5 py-1" aria-label="Codex is working">
            <span class="typing-dot"></span>
            <span class="typing-dot" style="animation-delay: 0.18s"></span>
            <span class="typing-dot" style="animation-delay: 0.36s"></span>
          </div>
        {/if}

        {#if thread.turns.length === 0}
          <p class="py-16 text-center text-sm text-surface-500">
            {threadId ? "This thread has no messages yet." : "Send a message to start a new thread."}
          </p>
        {/if}
      {/if}
    </div>
  </div>

  {#if turnPlan && turnPlan.steps.length > 0}
    <div class="mx-auto w-full max-w-3xl px-6">
      <div class="mb-2"><TurnPlanCard plan={turnPlan} /></div>
    </div>
  {/if}

  {#if goal}
    <div class="mx-auto w-full max-w-3xl px-6">
      <div
        class="mb-2 flex items-center gap-2 rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-1.5 text-xs"
        data-testid="goal-banner"
        title={goal.tokenBudget ? `${goal.tokensUsed.toLocaleString()} of ${goal.tokenBudget.toLocaleString()} tokens used` : undefined}
      >
        <Target size={12} class="shrink-0 {goal.status === 'active' ? 'text-primary-500' : 'text-surface-500'}" />
        <span class="shrink-0 font-medium text-surface-500">Goal · {GOAL_STATUS_LABEL[goal.status] ?? goal.status}</span>
        <span class="min-w-0 flex-1 truncate" title={goal.objective}>{goal.objective}</span>
        {#if goal.status !== "complete"}
          <TooltipButton
            label={goal.status === "active" ? "Pause goal" : "Resume goal"}
            onclick={toggleGoal}
            aria-label={goal.status === "active" ? "Pause goal" : "Resume goal"}
            class="shrink-0 opacity-60 hover:opacity-100"
          >
            {#if goal.status === "active"}<Pause size={12} />{:else}<Play size={12} />{/if}
          </TooltipButton>
        {/if}
        <TooltipButton label="Clear goal" onclick={clearGoal} aria-label="Clear goal" class="shrink-0 opacity-60 hover:opacity-100">
          <X size={12} />
        </TooltipButton>
      </div>
    </div>
  {/if}

  {#if notice}
    <div class="mx-auto w-full max-w-3xl px-6">
      <div class="card preset-tonal-warning mb-2 flex items-start gap-2 px-3 py-2 text-xs">
        <span class="min-w-0 flex-1">{notice}</span>
        <button type="button" onclick={() => (notice = null)} class="shrink-0 opacity-60 hover:opacity-100" aria-label="Dismiss notice">
          <X size={12} />
        </button>
      </div>
    </div>
  {/if}

  {#if queued.length > 0}
    <div class="mx-auto w-full max-w-3xl space-y-1 px-6 pb-2">
      {#each queued as entry, index (entry.id)}
        <QueuedMessageRow
          {entry}
          canSendNow={activeTurn !== null || index > 0}
          onSendNow={() => sendNow(entry)}
          onEdit={(input) => editQueued(entry, input)}
          onCancel={() => cancelQueued(entry)}
        />
      {/each}
    </div>
  {/if}

  {#if !threadId}
    <div class="mx-auto w-full max-w-3xl px-6 pb-2">
      <div class="flex items-center gap-1 rounded-lg border border-surface-200-800 bg-surface-100-900 p-1 text-xs">
        {#if workspaceId}
          <span class="px-2 text-surface-500">Workspace hub — shared notes and all member roots are writable</span>
        {:else}
          <span class="px-2 text-surface-500">Start in</span>
          <button
            type="button"
            class="rounded-md px-2.5 py-1.5 transition {newThreadLocation === 'project' ? 'bg-surface-50-950 font-medium shadow-sm' : 'text-surface-500 hover:text-surface-800-200'}"
            onclick={() => chooseNewThreadLocation("project")}
          >
            Project
          </button>
          <button
          type="button"
          disabled={!repoInfo?.isGitRepo || !codexHome}
          title={!repoInfo?.isGitRepo ? "This project is not a Git repository" : "Created under CODEX_HOME/worktrees-tmp and kept across restarts"}
          class="rounded-md px-2.5 py-1.5 transition disabled:cursor-not-allowed disabled:opacity-40 {newThreadLocation === 'temporary' ? 'bg-surface-50-950 font-medium shadow-sm' : 'text-surface-500 hover:text-surface-800-200'}"
          onclick={() => chooseNewThreadLocation("temporary")}
        >
          Temporary worktree
          </button>
          <button
          type="button"
          disabled={!repoInfo?.isGitRepo}
          title={!repoInfo?.isGitRepo ? "This project is not a Git repository" : "Create a named worktree under CODEX_HOME/worktrees"}
          class="rounded-md px-2.5 py-1.5 transition disabled:cursor-not-allowed disabled:opacity-40 {newThreadLocation === 'permanent' ? 'bg-surface-50-950 font-medium shadow-sm' : 'text-surface-500 hover:text-surface-800-200'}"
          onclick={() => chooseNewThreadLocation("permanent")}
        >
          Permanent worktree
          </button>
          {#if newThreadLocation === "permanent"}
            <span class="min-w-0 flex-1 truncate px-2 text-right font-mono text-[10px] text-surface-500" title={newThreadCwd}>
              {newThreadCwd}
            </span>
          {/if}
        {/if}
      </div>
    </div>
  {/if}

  <Composer
    bind:this={composer}
    busy={activeTurn !== null || starting}
    disabled={loading || error !== null}
    hasQuestions={threadQuestions.length > 0}
    plan={pendingPlan}
    cwd={workspaceId ? cwd : (thread?.cwd || cwd)}
    draftKey={threadId ? `${projectPath || cwd}#thread:${threadId}` : projectPath || cwd}
    history={messageHistory}
    projectKey={projectPath || cwd}
    {threadId}
    onSend={send}
    onInterrupt={interrupt}
    onCommand={runCommand}
    onReview={(target) => void review(target)}
    onImplementFresh={liveThreadId ? startPlanThread : undefined}
    {contextStats}
    {compacting}
    {subagentModelPolicy}
    {subagentReasoningEffortPolicy}
    threadModel={lastTurnModel}
    onSubagentPolicyChange={changeSubagentPolicy}
    onModelChange={(modelId) => (activeModel = modelId)}
  />
</div>

{#if panelView}
  <RightPanel
    view={panelView}
    parentThreadId={liveThreadId}
    {sideQuestions}
    changes={panelChanges}
    {gitChanges}
    {gitChangesLoading}
    {gitChangesError}
    onRefreshGitChanges={() => refreshGitChanges(true)}
    {contextStats}
    costUsd={estimateCost(tokenUsage, activeModel)}
    {activeModel}
    cwd={workspaceId ? cwd : (thread?.cwd || cwd)}
    onClose={() => (panelView = null)}
    onDataChanged={(data) => onDataChanged?.(data)}
    onImplementPlan={implementPlan}
    onImplementPlanFresh={liveThreadId ? implementPlanFresh : undefined}
    implementDisabled={activeTurn !== null || starting}
    onStopProcessTurn={(process) => {
      if (process.turnId && process.threadId) {
        interruptTurn(process.threadId, process.turnId).catch((cause) => {
          streamError = cause instanceof Error ? cause.message : String(cause);
        });
      }
    }}
  />
{/if}
</div>

<style>
  .typing-dot {
    width: 6px;
    height: 6px;
    border-radius: 9999px;
    background: color-mix(in oklab, currentColor 45%, transparent);
    animation: typing-bounce 1.1s ease-in-out infinite;
  }
  @keyframes typing-bounce {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.45;
    }
    30% {
      transform: translateY(-4px);
      opacity: 1;
    }
  }
</style>
