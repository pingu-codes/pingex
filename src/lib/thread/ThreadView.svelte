<script lang="ts">
import { ArrowDown, ChevronRight, Pause, Pencil, Play, Target, X } from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import { onDestroy, tick, untrack } from "svelte";
import { nameNewThread } from "$lib/app/appData.svelte";
import { openDialog } from "$lib/app/dialogs.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import Composer from "$lib/composer/Composer.svelte";
import type { SlashCommandId } from "$lib/composer/slashCommands";
import RightPanel, { type PanelView } from "$lib/panels/RightPanel.svelte";
import { runsFor } from "$lib/services/agentRuns.svelte";
import {
  addThreadBranch,
  clearThreadGoal,
  compactThread,
  copyText,
  forkThread,
  getThreadGoal,
  gitChangesSummary,
  gitRepoInfo,
  gitWorktreeAdd,
  interruptTurn,
  killAgentRun,
  openInZed,
  revealInFinder,
  reviewLocalDiff,
  rollbackThread,
  setThreadBranchEditTurn,
  setThreadGoal,
  setThreadGoalStatus,
  startReview,
  startThread,
  startTurn,
  updateSubagentPolicy,
} from "$lib/services/api";
import { approvals, elicitations, turnPlans, userInputRequests } from "$lib/services/codexEvents.svelte";
import { processes } from "$lib/services/processes.svelte";
import ApprovalCard from "$lib/thread/ApprovalCard.svelte";
import { requestAutoName } from "$lib/thread/autoName";
import { contextStats as deriveContextStats } from "$lib/thread/contextUsage";
import DiscardQueuedDialog from "$lib/thread/DiscardQueuedDialog.svelte";
import ElicitationCard from "$lib/thread/ElicitationCard.svelte";
import FloatingMenu from "$lib/thread/FloatingMenu.svelte";
import { collectFileChanges } from "$lib/thread/fileChanges";
import { cwdBelongsTo } from "$lib/thread/handoff";
import { messageText, messageTitle } from "$lib/thread/messageText";
import { groupForTurn, isPendingEditTurn, versionsForTurn } from "$lib/thread/messageVersions";
import { planText } from "$lib/thread/planText";
import QuestionCard from "$lib/thread/QuestionCard.svelte";
import QueuedMessageRow from "$lib/thread/QueuedMessageRow.svelte";
import ReasoningBlock from "$lib/thread/ReasoningBlock.svelte";
import ReplaceGoalDialog from "$lib/thread/ReplaceGoalDialog.svelte";
import RewindThreadDialog from "$lib/thread/RewindThreadDialog.svelte";
import { nextFollowing, recallScroll, rememberScroll } from "$lib/thread/scrollPositions";
import { attachSession, draftSession, openSession, releaseSession } from "$lib/thread/sessions.svelte";
import TurnPlanCard from "$lib/thread/TurnPlanCard.svelte";
import type { ThreadSession } from "$lib/thread/threadSession.svelte";
import { ensureTurn, upsertItem } from "$lib/thread/threadStream";
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
  GitRepoInfo,
  QueuedSubmission,
  ReviewTarget,
  SideQuestion,
  SubagentDetail,
  SubagentPolicy,
  ThreadBranch,
  ThreadItem,
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
  threadBranches = [],
  onThreadCreated,
  onDataChanged,
  onCommand,
  onSelectThread,
  onOpenSubagent,
  onSelectVersion,
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
  /** Every message-version branch; the bubbles find their own in here. */
  threadBranches?: ThreadBranch[];
  onThreadCreated?: (id: string, cwd: string) => void;
  onDataChanged?: (data: BootstrapData) => void;
  /** Thread-level slash commands from the composer (new, fork, archive, rename). */
  onCommand?: (command: SlashCommandId, threadId: string | null) => void;
  onSelectThread?: (id: string) => void;
  /** Open a subagent thread; the full detail lets the app navigate to
   *  subagents that bootstrap hasn't picked up yet (e.g. mid plan mode). */
  onOpenSubagent?: (agent: SubagentDetail) => void;
  /** Open the thread holding another version of an edited message. */
  onSelectVersion?: (threadId: string) => void;
} = $props();

/**
 * The session for the thread on show — its transcript, queue and stream state,
 * which outlive this view while the thread has work in flight. A draft gets a
 * session of its own that joins the registry once the thread exists.
 */
function bind(id: string | null): ThreadSession {
  return id ? openSession(id) : draftSession(cwd);
}
let session = $state.raw<ThreadSession>(bind(untrack(() => threadId)));
$effect(() => {
  const id = threadId;
  untrack(() => {
    // The draft's session takes the created thread's id itself, so it stays.
    if (session.id === id) return;
    releaseSession(session);
    session = bind(id);
  });
});
onDestroy(() => releaseSession(untrack(() => session)));
const thread = $derived(session.thread);
const liveThreadId = $derived(session.id);
const activeTurn = $derived(session.activeTurn);
const loading = $derived(session.loading);
const error = $derived(session.error);
const streamError = $derived(session.streamError);
const notice = $derived(session.notice);
const goal = $derived(session.goal);
const starting = $derived(session.starting);
const compacting = $derived(session.compacting);
const tokenUsage = $derived(session.tokenUsage);
const subagentModelPolicy = $derived(session.subagentModelPolicy);
const subagentReasoningEffortPolicy = $derived(session.subagentReasoningEffortPolicy);
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
// Surface stream errors as dismissable, auto-expiring toasts (see ToastHost).
$effect(() => {
  if (!streamError) return;
  toastError(streamError);
  session.streamError = null;
});
let scroller: HTMLElement | null = null;
/** Thread whose remembered scroll offset has been applied to `scroller`. */
let restoredScrollFor: string | null = null;
/** Whether the transcript tracks the live bottom. Only user scrolling changes it;
 *  position is a poor proxy for intent while content streams in fast. */
let following = $state(true);
/** Set right before we assign `scrollTop` so the resulting scroll event is not
 *  mistaken for the user's. One-shot, deliberately not reactive. */
let programmaticScroll = false;
let lastScrollTop = 0;
let panelView = $state<PanelView | null>(null);
let composer = $state<{
  implementPlan: () => void;
  implementPlanFresh: (plan?: string | null) => void;
  appSubagentsChoice: () => boolean | null;
  harnessChoice: () => "codex" | "claude" | null;
  openReviewPicker: () => void;
  restoreText: (text: string) => void;
  turnOptions: () => TurnOptions | undefined;
  isEmpty: () => boolean;
} | null>(null);
/** Model the composer will run turns on — priced for the usage estimate. */
let activeModel = $state<string | null>(null);
type NewThreadLocation = "project" | "temporary" | "permanent";
let newThreadLocation = $state<NewThreadLocation>("project");
let newThreadCwd = $state("");
let repoInfo = $state<GitRepoInfo | null>(null);

$effect(() => {
  if (threadId) return;
  const dir = projectPath || cwd;
  gitRepoInfo(dir)
    .then((info) => {
      repoInfo = info;
    })
    .catch(() => {
      repoInfo = null;
    });
});

function chooseNewThreadLocation(location: NewThreadLocation) {
  if (location === "permanent") {
    openDialog(CreateWorktreeDialog, {
      codexHome,
      repoDir: repoInfo?.root ?? (projectPath || cwd),
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
  ...session.subagents.map((detail) => ({ ...detail, source: "codex" as const })),
  ...appSubagentDetails,
]);

// Follow the transcript as it streams: `revision` moves on every applied event.
let seenRevision = untrack(() => session.revision);
$effect(() => {
  const revision = session.revision;
  if (revision === seenRevision) return;
  seenRevision = revision;
  maybeScroll();
});

function maybeScroll(force = false) {
  const element = scroller;
  if (!element) return;
  if (force) following = true;
  if (!following) return;
  requestAnimationFrame(() => scrollProgrammatically(element, element.scrollHeight));
}

/** Assign `scrollTop` without the resulting scroll event flipping `following`.
 *  When nothing moves no event fires, so drop the guard at once rather than let
 *  it swallow the next real user scroll. */
function scrollProgrammatically(element: HTMLElement, top: number) {
  const before = element.scrollTop;
  programmaticScroll = true;
  element.scrollTop = top;
  if (element.scrollTop === before) programmaticScroll = false;
}

function onScroll() {
  const element = scroller;
  if (!element) return;
  if (programmaticScroll) {
    programmaticScroll = false;
  } else {
    following = nextFollowing(following, lastScrollTop, element);
  }
  lastScrollTop = element.scrollTop;
  if (liveThreadId) rememberScroll(liveThreadId, element);
}

/** A wheel-up and a programmatic scroll-down can land in the same frame, so the
 *  net scroll event alone may read as "moved down"; detach on the intent itself. */
function onWheel(event: WheelEvent) {
  if (event.deltaY < 0) following = false;
}

// Switching threads remounts this view, so the scroller opens at the top; put
// it back where this thread was left once the transcript has rendered.
$effect(() => {
  const id = liveThreadId;
  if (loading || !thread || !id || restoredScrollFor === id) return;
  restoredScrollFor = id;
  const saved = recallScroll(id);
  if (!saved) return;
  tick().then(() => {
    requestAnimationFrame(() => {
      const element = scroller;
      if (!element || liveThreadId !== id) return;
      following = saved.atBottom;
      scrollProgrammatically(element, saved.atBottom ? element.scrollHeight : saved.top);
      lastScrollTop = element.scrollTop;
    });
  });
});

/**
 * The thread id to run work on, creating the thread first when this view is
 * still an unsent draft. Callers own `starting` around it: it leaves the draft
 * running under its new id either way.
 */
async function ensureLiveThread(): Promise<string> {
  if (session.id) return session.id;
  const newCwd = await ensureNewThreadCwd();
  const created = await startThread(
    newCwd,
    workspaceId,
    composer?.appSubagentsChoice() ?? null,
    composer?.harnessChoice() ?? null,
  );
  if (session.thread) session.thread.harness = created.harness ?? null;
  // The draft is now a real thread: it runs under its id from here, and is
  // retained if the view leaves mid-turn.
  attachSession(session, created.id);
  onThreadCreated?.(created.id, created.cwd ?? newCwd);
  return created.id;
}

/** Send from the composer. A draft becomes a real thread (and gets its name)
 *  first; from there the session owns queueing, the optimistic bubble and the
 *  turn. Returns whether the message is accounted for. */
async function send(input: UserInputPart[], options?: TurnOptions): Promise<boolean> {
  if (!session.id && !session.activeTurn && !session.starting) {
    session.starting = true;
    try {
      const id = await ensureLiveThread();
      // Name the thread off its opening message before the turn is even started:
      // nothing else can title it until its rollout persists, so waiting here is
      // what leaves the sidebar reading "Untitled thread" for the whole turn.
      nameNewThread(id, messageTitle(input));
      const text = messageText(input);
      if (text.trim()) requestAutoName(id, "seed", text);
    } catch (cause) {
      session.streamError = cause instanceof Error ? cause.message : String(cause);
      return false;
    } finally {
      session.starting = false;
    }
  }
  const sent = session.send(input, options);
  // The optimistic bubble is already in the transcript; follow it.
  maybeScroll(true);
  return sent;
}

/** Editing a past user message forks the thread to just before that turn and
 *  sends the edit on the fork, so the original and every later edit survive
 *  as versions the bubble can page between. The fork's session is primed and
 *  sent to before the view moves there, so the edit is already streaming when
 *  the remounted view picks the session up. */
async function submitEdit(turn: Turn, parts: UserInputPart[]) {
  if (!thread || !liveThreadId || turn.id.startsWith("local-") || activeTurn) return;
  const threadIdAtEdit = liveThreadId;
  const index = thread.turns.findIndex((candidate) => candidate.id === turn.id);
  if (index === -1) return;
  session.streamError = null;
  session.starting = true;
  try {
    const forked = await forkThread(threadIdAtEdit, turn.id);
    onDataChanged?.(await addThreadBranch(threadIdAtEdit, forked.id, turn.id, index));
    const forkSession = openSession(forked.id);
    try {
      await forkSession.load();
      const sent = await forkSession.send(parts, composer?.turnOptions());
      if (!sent) throw new Error(forkSession.streamError ?? "Could not send the edited message");
      const editTurnId = forkSession.thread?.turns.at(-1)?.id;
      if (editTurnId && !editTurnId.startsWith("local-")) {
        setThreadBranchEditTurn(forked.id, editTurnId).catch(() => {});
      }
    } finally {
      // Retained while its turn runs; the view that opens it takes it over.
      releaseSession(forkSession);
    }
    onSelectVersion?.(forked.id);
  } catch (cause) {
    session.streamError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    session.starting = false;
  }
}

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

/** Take a message off the queue: back into an empty composer, or, when that
 *  would clobber typed text, only after the user agrees to lose it. */
async function cancelQueued(entry: QueuedSubmission) {
  const text = queuedPreview(entry.input);
  if (composer?.isEmpty()) {
    await session.queue.remove(entry);
    composer.restoreText(text);
    return;
  }
  if (!(await openDialog(DiscardQueuedDialog, { preview: text }))) return;
  await session.queue.remove(entry);
}

function queuedPreview(input: UserInputPart[]) {
  return input
    .map((part) => (part.type === "text" ? (part.text ?? "") : `@${part.name}`))
    .join("")
    .trim();
}

const interrupt = () => session.interrupt();

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
  session.starting = true;
  session.streamError = null;
  const dir = workspaceId ? cwd : thread?.cwd || cwd;
  try {
    const created = await startThread(
      dir,
      workspaceId,
      composer?.appSubagentsChoice() ?? null,
      thread?.harness === "claude" ? "claude" : null,
    );
    await startTurn(created.id, input, options);
    // Adopting the new thread re-drives the load effect, so the view swaps to it.
    onThreadCreated?.(created.id, created.cwd ?? dir);
    // The plan itself is this thread's opening message, so it names it.
    nameNewThread(created.id, messageTitle(input));
    const plan = messageText(input);
    if (plan.trim()) requestAutoName(created.id, "seed", plan);
  } catch (cause) {
    session.streamError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    session.starting = false;
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
  session.compacting = true;
  session.streamError = null;
  try {
    await compactThread(liveThreadId);
  } catch (cause) {
    session.compacting = false;
    session.streamError = cause instanceof Error ? cause.message : String(cause);
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
  session.streamError = null;
  const fresh = !liveThreadId;
  if (fresh) session.starting = true;
  try {
    const turn = await startReview(await ensureLiveThread(), target);
    if (thread) {
      ensureTurn(thread.turns, turn.id).status = turn.status ?? "inProgress";
      maybeScroll(true);
    }
  } catch (cause) {
    session.streamError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    if (fresh) session.starting = false;
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
    session.streamError = cause instanceof Error ? cause.message : String(cause);
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
      session.notice = "/review can't start while Codex is working — stop the current turn first.";
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
  session.streamError = message ?? `/${command} needs an open thread.`;
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
      session.notice = "No goal is set — /goal <objective> sets one.";
      return;
    }
    try {
      if (argument) {
        await clearThreadGoal(current);
        session.setGoal(null);
        session.notice = "Goal cleared.";
      } else {
        const held = await getThreadGoal(current);
        session.setGoal(held);
        session.notice = held
          ? `Goal (${held.status}): ${held.objective}`
          : "No goal is set — /goal <objective> sets one.";
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
  if (!fresh) {
    // The thread may already be working towards something — never overwrite
    // it silently. (This also covers the goal-started draft, which still
    // looks like a draft because adopting the thread does not remount.)
    const existing = session.goal ?? (await getThreadGoal(current).catch(() => null));
    if (existing && existing.objective !== objective) {
      const ok = await openDialog(ReplaceGoalDialog, { current: existing.objective, next: objective });
      if (!ok) {
        if (typed) composer?.restoreText(typed);
        return;
      }
    }
  }
  if (fresh) session.starting = true;
  try {
    const id = await ensureLiveThread();
    const set = await setThreadGoal(id, objective);
    session.setGoal(set);
    session.notice = `Goal set: ${set.objective}`;
    // A goal-only thread has no turn to name it from, so the objective titles
    // it: shown at once, then refined by the namer.
    if (fresh) {
      nameNewThread(id, messageTitle([{ type: "text", text: objective }]));
      requestAutoName(id, "seed", objective);
    }
  } catch (cause) {
    failCommand("goal", typed, cause instanceof Error ? cause.message : String(cause));
  } finally {
    if (fresh) session.starting = false;
  }
}

/** Pause a running goal, or resume a paused (or otherwise stalled) one. */
async function toggleGoal() {
  if (!goal || !liveThreadId) return;
  const status = goal.status === "active" ? "paused" : "active";
  try {
    session.setGoal({ ...goal, ...(await setThreadGoalStatus(liveThreadId, status)), objective: goal.objective });
  } catch (cause) {
    toastError(cause instanceof Error ? cause.message : String(cause));
  }
}

async function clearGoal() {
  if (!liveThreadId) return;
  try {
    await clearThreadGoal(liveThreadId);
    session.setGoal(null);
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

/** Badge preset for each goal status, so the banner shows state at a glance. */
const GOAL_STATUS_PRESET: Record<string, string> = {
  active: "preset-filled-primary-500",
  paused: "preset-tonal-surface",
  blocked: "preset-tonal-error",
  usageLimited: "preset-tonal-warning",
  budgetLimited: "preset-tonal-error",
  complete: "preset-tonal-success",
};

/** Inline goal-objective editing in the banner. */
let editingGoal = $state(false);
let goalDraft = $state("");

function startGoalEdit() {
  if (!goal) return;
  goalDraft = goal.objective;
  editingGoal = true;
}

async function saveGoalEdit() {
  const next = goalDraft.trim();
  if (!liveThreadId || !next || next === goal?.objective) {
    editingGoal = false;
    return;
  }
  try {
    session.setGoal(await setThreadGoal(liveThreadId, next));
    editingGoal = false;
  } catch (cause) {
    toastError(cause instanceof Error ? cause.message : String(cause));
  }
}

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
    session.notice = "No response to copy yet.";
    return;
  }
  try {
    await copyText(last.text);
    session.notice = "Copied the last response as markdown.";
  } catch (cause) {
    session.streamError = cause instanceof Error ? cause.message : String(cause);
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
    session.notice = "Nothing to export yet.";
    return;
  }
  try {
    await copyText(sections.join("\n\n"));
    session.notice = "Copied the conversation as markdown.";
  } catch (cause) {
    session.streamError = cause instanceof Error ? cause.message : String(cause);
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
  session.streamError = null;
  session.starting = true;
  try {
    await rollbackThread(threadIdAtUndo, dropped);
    if (!thread || liveThreadId !== threadIdAtUndo) return;
    thread.turns = thread.turns.slice(0, thread.turns.length - dropped);
  } catch (cause) {
    session.streamError = cause instanceof Error ? cause.message : String(cause);
  } finally {
    session.starting = false;
  }
}

function changeSubagentPolicy(modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null) {
  session.subagentModelPolicy = modelPolicy;
  session.subagentReasoningEffortPolicy = effortPolicy;
  if (!liveThreadId) return;
  updateSubagentPolicy(liveThreadId, modelPolicy, effortPolicy).catch((cause) => {
    session.streamError = cause instanceof Error ? cause.message : String(cause);
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
      onOpenZed={() => openInZed(thread?.cwd || cwd).catch((cause) => (session.streamError = cause instanceof Error ? cause.message : String(cause)))}
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
        session.refreshSubagents();
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
  <div class="relative flex min-h-0 flex-1 flex-col">
    <div
      class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto"
      bind:this={scroller}
      onscroll={onScroll}
      onwheel={onWheel}
    >
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
          {#each thread.turns as turn, turnIndex (turn.id)}
            {@const parts = splitTurn(turn)}
            {@const collapseDiffs = turnDiffCount(turn) > 1}
            {@const liveSegment = turn.status === "inProgress" ? parts.body.at(-1) : undefined}
            {#each parts.users as item (item.id)}
              <UserMessageBubble
                {item}
                cwd={thread?.cwd || cwd}
                editable={!!liveThreadId && !turn.id.startsWith("local-") && !activeTurn && thread?.harness !== "claude"}
                versions={versionsForTurn(
                  isPendingEditTurn(liveThreadId, turnIndex, threadBranches) ? "" : turn.id,
                  threadBranches,
                  liveThreadId,
                )}
                onSubmitEdit={(parts) => submitEdit(turn, parts)}
                {onSelectVersion}
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
              {@const steer = turn.error.misalignment?.steer?.message}
              <div class="card preset-tonal-error space-y-2 p-3 text-xs">
                <div>{turn.error.message}</div>
                {#if turn.error.misalignment?.detailedExplanation}
                  <p class="whitespace-pre-wrap opacity-90">{turn.error.misalignment.detailedExplanation}</p>
                {/if}
                {#if steer}
                  <button
                    type="button"
                    class="btn btn-sm preset-tonal"
                    disabled={Boolean(session.activeTurn)}
                    onclick={() => void send([{ type: "text", text: steer }])}
                  >
                    Continue with suggested steer
                  </button>
                {/if}
              </div>
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
    {#if !following && !loading && thread}
      <button
        type="button"
        class="btn btn-sm preset-filled absolute bottom-3 left-1/2 -translate-x-1/2 gap-1 rounded-full shadow-lg"
        onclick={() => maybeScroll(true)}
      >
        <ArrowDown class="size-3.5" />
        Jump to latest
      </button>
    {/if}
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
        <span class="shrink-0 font-medium text-surface-500">Goal</span>
        <span
          class="badge shrink-0 px-1.5 py-0 text-[10px] {GOAL_STATUS_PRESET[goal.status] ?? 'preset-tonal-surface'}"
          data-testid="goal-status"
        >
          {GOAL_STATUS_LABEL[goal.status] ?? goal.status}
        </span>
        {#if editingGoal}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="input h-6 min-w-0 flex-1 px-2 text-xs"
            bind:value={goalDraft}
            aria-label="Goal objective"
            autofocus
            onkeydown={(event) => {
              if (event.key === "Enter") void saveGoalEdit();
              if (event.key === "Escape") editingGoal = false;
            }}
          />
        {:else}
          <span class="min-w-0 flex-1 truncate" title={goal.objective}>{goal.objective}</span>
          <TooltipButton label="Edit goal" onclick={startGoalEdit} aria-label="Edit goal" class="shrink-0 opacity-60 hover:opacity-100">
            <Pencil size={12} />
          </TooltipButton>
        {/if}
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
        <button type="button" onclick={() => (session.notice = null)} class="shrink-0 opacity-60 hover:opacity-100" aria-label="Dismiss notice">
          <X size={12} />
        </button>
      </div>
    </div>
  {/if}

  {#if session.queue.entries.length > 0}
    <div class="mx-auto w-full max-w-3xl space-y-1 px-6 pb-2">
      {#each session.queue.entries as entry, index (entry.id)}
        <QueuedMessageRow
          {entry}
          canSendNow={activeTurn !== null || index > 0}
          onSendNow={() => session.queue.promote(entry)}
          onEdit={(input) => session.queue.edit(entry, input)}
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
    threadHarness={thread?.harness === "claude" ? "claude" : threadId ? "codex" : null}
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
    onLiveSettingsChange={(settings) => {
      if (session.activeTurn) void session.updateLiveSettings(settings);
    }}
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
          session.streamError = cause instanceof Error ? cause.message : String(cause);
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
