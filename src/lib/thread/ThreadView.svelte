<script lang="ts">
  import { ArrowDown, ChevronRight, X } from "@lucide/svelte";
  import { Collapsible } from "@skeletonlabs/skeleton-svelte";
  import { onDestroy, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { openDialog } from "$lib/app/dialogs.svelte";
  import BrowserApprovalCard from "$lib/browser/BrowserApprovalCard.svelte";
  import { approvalsFor, labelBrowserThread } from "$lib/browser/browserStore.svelte";
  import { RefreshScheduler } from "$lib/changes/changesPage";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import Segmented, { type SegmentedOption } from "$lib/components/Segmented.svelte";
  import TypingDots from "$lib/components/TypingDots.svelte";
  import Composer from "$lib/composer/Composer.svelte";
  import type { SlashCommandId } from "$lib/composer/slashCommands";
  import RightPanel, { type PanelView } from "$lib/panels/RightPanel.svelte";
  import { isPromptPartsTextUnsupported } from "$lib/services/api";
  import { approvals, elicitations, turnPlans, userInputRequests } from "$lib/services/codexEvents.svelte";
  import { commands } from "$lib/services/homeRouting";
  import {
    processes,
    type RunningProcess,
    reconcileBackground,
    stopAllBackground,
    terminateProcess,
  } from "$lib/services/processes.svelte";
  import { invalidate, query } from "$lib/services/queryCache.svelte";
  import ApprovalCard from "$lib/thread/ApprovalCard.svelte";
  import { contextStats as deriveContextStats } from "$lib/thread/contextUsage";
  import { type DraftLocation, DraftThread, editAsBranch, startThreadWith } from "$lib/thread/draftThread.svelte";
  import ElicitationCard from "$lib/thread/ElicitationCard.svelte";
  import FloatingMenu from "$lib/thread/FloatingMenu.svelte";
  import { collectFileChanges } from "$lib/thread/fileChanges";
  import GoalBanner from "$lib/thread/GoalBanner.svelte";
  import { goalCommands } from "$lib/thread/goalCommands";
  import { cwdBelongsTo } from "$lib/thread/handoff";
  import { messageText } from "$lib/thread/messageText";
  import { isPendingEditTurn, versionsForTurn } from "$lib/thread/messageVersions";
  import { planText } from "$lib/thread/planText";
  import QuestionCard from "$lib/thread/QuestionCard.svelte";
  import QueuedMessageRow from "$lib/thread/QueuedMessageRow.svelte";
  import { queuedText } from "$lib/thread/queueEntries";
  import ReasoningBlock from "$lib/thread/ReasoningBlock.svelte";
  import SystemPromptSection from "$lib/thread/SystemPromptSection.svelte";
  import { openSession, releaseSession } from "$lib/thread/sessions.svelte";
  import TurnPlanCard from "$lib/thread/TurnPlanCard.svelte";
  import type { ThreadSession } from "$lib/thread/threadSession.svelte";
  import { upsertItem } from "$lib/thread/threadStream";
  import { TranscriptScroll } from "$lib/thread/transcriptScroll.svelte";
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
  import { splitWorktreeContext } from "$lib/thread/worktreeContext";
  import { toastError } from "$lib/toaster";
  import type {
    BootstrapData,
    FileUpdateChange,
    PromptPart,
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
  import { can, harnessKind } from "$lib/utils/capabilities";
  import { errorMessage } from "$lib/utils/errors";
  import CreateWorktreeDialog from "$lib/worktrees/CreateWorktreeDialog.svelte";

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
    onOpenChanges,
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
    onThreadCreated?: (id: string, cwd: string, inTempWorktree?: boolean) => void;
    onDataChanged?: (data: BootstrapData) => void;
    /** Thread-level slash commands from the composer (new, fork, archive, rename). */
    onCommand?: (command: SlashCommandId, threadId: string | null) => void;
    /** Open the thread's Changes page, on a file or a segment. */
    onOpenChanges?: (threadId: string, options: { path?: string | null; segment?: string | null }) => void;
    onSelectThread?: (id: string) => void;
    /** Open a subagent thread; the full detail lets the app navigate to
     *  subagents that bootstrap hasn't picked up yet (e.g. mid plan mode). */
    onOpenSubagent?: (agent: SubagentDetail) => void;
    /** Open the thread holding another version of an edited message. */
    onSelectVersion?: (threadId: string) => void;
  } = $props();

  /** Where an unsent draft will start, and how it becomes a thread. */
  const draft = new DraftThread({
    threadId: () => threadId,
    cwd: () => cwd,
    projectPath: () => projectPath,
    workspaceId: () => workspaceId,
    session: () => session,
    onCreated: (id, createdCwd, inTempWorktree) => onThreadCreated?.(id, createdCwd, inTempWorktree),
  });

  /**
   * The session for the thread on show — its transcript, queue and stream state,
   * which outlive this view while the thread has work in flight. A draft gets a
   * session of its own that creates the thread when work needs one, and joins
   * the registry once it exists.
   */
  function bind(id: string | null): ThreadSession {
    return id ? openSession(id) : draft.session();
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
  /** Show a failed action's error on the thread. */
  const fail = (cause: unknown) => (session.streamError = errorMessage(cause));
  // Sends that bypass the composer's text (a stranded answer, a steer, a drain
  // that lost its options) still run with the composer's choices.
  $effect(() => {
    const current = session;
    current.turnOptions = () => untrack(() => composer?.turnOptions());
  });
  // A steer the turn ended without taking goes back into an empty composer;
  // with text already there it is queued instead, so nothing typed is lost.
  $effect(() => {
    const current = session;
    const returned = (inputs: UserInputPart[][]) => {
      if (composer?.isEmpty()) {
        composer.restoreText(inputs.map((input) => queuedText(input)).join("\n\n"));
        session.notice = "The turn ended before it took your message, so it's back in the composer.";
      } else {
        for (const input of inputs) void current.queue.add(input);
      }
    };
    current.onSteersReturned = returned;
    return () => {
      if (current.onSteersReturned === returned) current.onSteersReturned = null;
    };
  });
  // The harness changed mode by itself (Claude leaves plan mode when a plan is
  // approved); the pill follows once, then the event is spent.
  $effect(() => {
    const mode = session.collaborationMode;
    if (!mode || !composer) return;
    session.collaborationMode = null;
    composer.syncPlanMode(mode === "plan");
  });
  $effect(() => {
    const mode = session.permissionMode;
    if (!mode || !composer) return;
    session.permissionMode = null;
    composer.syncPermissionMode(mode);
  });
  const thread = $derived(session.thread);
  /** Folded "Worked" segments the user has opened, by `workKey`. Skeleton's
   *  Collapsible keeps closed content mounted, and a long thread's folded work
   *  (markdown, command output, highlighted diffs) is most of its DOM, so it is
   *  only rendered while open. */
  const openWork = new SvelteSet<string>();
  const workKey = (turnId: string, items: { id: string }[]) => `${turnId}:${items[0]?.id ?? ""}`;
  const liveThreadId = $derived(session.id);
  const activeTurn = $derived(session.activeTurn);
  const loading = $derived(session.loading);
  const error = $derived(session.error);
  const starting = $derived(session.starting);
  /** What the thread last ran on, so a turn sent before the model list loads
   *  still carries full collaboration-mode settings. */
  const lastTurnModel = $derived([...(thread?.turns ?? [])].reverse().find((turn) => turn.model)?.model ?? null);
  // Surface stream errors as dismissable, auto-expiring toasts (see ToastHost).
  $effect(() => {
    const streamError = session.streamError;
    if (!streamError) return;
    toastError(streamError);
    session.streamError = null;
  });
  const transcript = new TranscriptScroll(() => liveThreadId);
  let panelView = $state<PanelView | null>(null);
  /** Whether the system prompt's full text is shown inline at the top of the
   *  scrollback. Toggled from the status panel; fetched once per thread and
   *  cached here since it can only come from Codex's rollout file. */
  let showSystemPromptInThread = $state(false);
  let systemPromptParts = $state<PromptPart[] | null>(null);
  let systemPromptPartsFor: string | null = null;

  function toggleSystemPromptInThread() {
    if (showSystemPromptInThread) {
      showSystemPromptInThread = false;
      return;
    }
    const id = liveThreadId;
    if (!id) return;
    showSystemPromptInThread = true;
    if (systemPromptPartsFor === id && systemPromptParts) return;
    systemPromptPartsFor = id;
    commands
      .readPromptPartsText(id)
      .then((parts) => {
        if (systemPromptPartsFor === id) systemPromptParts = parts;
      })
      .catch((cause) => {
        if (systemPromptPartsFor !== id) return;
        if (!isPromptPartsTextUnsupported(cause)) {
          fail(cause);
        }
        showSystemPromptInThread = false;
        systemPromptPartsFor = null;
      });
  }
  let composer = $state<ReturnType<typeof Composer> | null>(null);
  /** Model the composer will run turns on — priced for the usage estimate. */
  let activeModel = $state<string | null>(null);
  const notGitRepo = "This project is not a Git repository";
  const repoInfo = $derived(draft.repoInfo);
  const newThreadLocations = $derived<SegmentedOption<DraftLocation>[]>([
    { value: "project", label: "Project" },
    {
      value: "temporary",
      label: "Temporary worktree",
      disabled: !repoInfo?.isGitRepo,
      title: repoInfo?.isGitRepo ? "Created under CODEX_HOME/worktrees-tmp and kept across restarts" : notGitRepo,
    },
    {
      value: "permanent",
      label: "Permanent worktree",
      disabled: !repoInfo?.isGitRepo,
      title: repoInfo?.isGitRepo ? "Create a named worktree under CODEX_HOME/worktrees" : notGitRepo,
    },
  ]);

  function chooseNewThreadLocation(location: DraftLocation) {
    if (location === "permanent") {
      openDialog(CreateWorktreeDialog, {
        codexHome,
        repoDir: draft.repoDir,
        submit: (path: string, branch: WorktreeBranchRequest) => draft.addWorktree(path, branch),
      });
      return;
    }
    draft.startIn(location);
  }

  // Follow the transcript as it streams: `revision` moves on every applied event.
  let seenRevision = untrack(() => session.revision);
  $effect(() => {
    const revision = session.revision;
    if (revision === seenRevision) return;
    seenRevision = revision;
    transcript.follow();
  });

  // Switching threads remounts this view, so the scroller opens at the top; put
  // it back where this thread was left once the transcript has rendered.
  $effect(() => {
    const id = liveThreadId;
    if (loading || !thread || !id) return;
    transcript.restore(id);
  });

  /** Send from the composer; a draft becomes its thread first. Returns
   *  whether the message is accounted for. */
  function send(input: UserInputPart[], options?: TurnOptions): Promise<boolean> {
    const sent = draft.send(session, input, options);
    // The optimistic bubble is already in the transcript; follow it.
    transcript.follow(true);
    return sent;
  }

  /** Send into the running turn (Enter mid-turn). Falls back to queueing when
   *  the turn cannot take it; the session decides. */
  function steer(input: UserInputPart[], options?: TurnOptions): Promise<boolean> {
    const sent = session.steer(input, options);
    transcript.follow(true);
    return sent;
  }

  /** Editing a past user message sends the edit on a new Branch, so the
   *  original and every later edit survive as Versions the bubble can page
   *  between. The Branch is already streaming when the view moves there. */
  async function submitEdit(turn: Turn, parts: UserInputPart[]) {
    const branch = await editAsBranch(session, turn, parts, (data) => onDataChanged?.(data));
    if (branch) onSelectVersion?.(branch);
  }

  const threadApprovals = $derived(approvals.list.filter((approval) => approval.threadId === liveThreadId));
  const browserApprovals = $derived(liveThreadId ? approvalsFor(liveThreadId) : []);
  const browserUse = $derived(can(thread?.harness, "browserUse"));
  const backgroundTerminals = $derived(can(thread?.harness, "backgroundTerminals"));
  // Commands that outlived their turn, including ones started before a reload.
  $effect(() => {
    const id = liveThreadId;
    if (id && backgroundTerminals) untrack(() => void reconcileBackground(id));
  });
  function stopProcess(process: RunningProcess) {
    terminateProcess(process).catch(fail);
  }
  // Name the thread's Pingex tab group in Chrome after the thread.
  $effect(() => {
    if (browserUse && liveThreadId && thread?.name) labelBrowserThread(liveThreadId, thread.name);
  });
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
    if (browserApprovals.length > 0) return false;
    if (threadElicitations.length > 0) return false;
    const last = activeTurn.items.at(-1);
    return !(last?.type === "agentMessage" && last.streaming);
  });

  /** Take a message off the queue: back into an empty composer, or, when that
   *  would clobber typed text, only after the user agrees to lose it. */
  async function cancelQueued(entry: QueuedSubmission) {
    const text = queuedText(entry.input);
    if (composer?.isEmpty()) {
      await session.queue.remove(entry);
      composer.restoreText(text);
      return;
    }
    const confirmed = await openDialog(ConfirmDialog, {
      title: "Discard queued message",
      body: "The composer already has text, so this message can't be moved back into it. Removing it from the queue discards it for good.",
      detail: text,
      confirmLabel: "Discard",
      cancelLabel: "Keep",
    });
    if (!confirmed) return;
    await session.queue.remove(entry);
  }

  const allItems = $derived((thread?.turns ?? []).flatMap((turn) => turn.items));
  /** Prior prompts, oldest first, for the composer's ↑/↓ recall. */
  const messageHistory = $derived.by(() => {
    const out: string[] = [];
    for (const item of allItems) {
      if (item.type !== "userMessage" || !item.content) continue;
      const text = messageText(splitWorktreeContext(item.content as UserInputPart[]).content).trim();
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
  // Latest diff per changed file across the whole thread, in first-touched order:
  // the Changes summary's stand-in when the thread works outside Git.
  const outputChanges = $derived(collectFileChanges(allItems));
  /** Where the thread works: a workspace's hub, else the thread's own directory. */
  const workDir = $derived(workspaceId ? cwd : thread?.cwd || cwd);

  /** The thread's changes for the overview, the same read the Changes page
   *  opens on. A finished turn and window focus mark it stale
   *  (`app/listeners.svelte.ts`); while the agent works, each edit or command
   *  it finishes does. */
  const changesQuery = query("gitThreadChanges", () =>
    liveThreadId ? [liveThreadId, thread?.cwd ?? null, "branch" as const] : null,
  );
  const threadChanges = $derived(changesQuery.value ?? null);
  const changesRefresh = new RefreshScheduler(() => invalidate({ names: ["gitThreadChanges"] }));
  onDestroy(() => changesRefresh.dispose());
  const finishedWork = $derived(
    (activeTurn?.items ?? []).filter(
      (item) => (item.type === "fileChange" || item.type === "commandExecution") && item.status !== "inProgress",
    ).length,
  );
  $effect(() => {
    if (finishedWork > 0) untrack(() => changesRefresh.poke());
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
    const dir = workDir;
    try {
      await startThreadWith({ cwd: dir, workspaceId, harness: harnessKind(thread?.harness) }, input, {
        options,
        // Adopting the new thread re-drives the load effect, so the view swaps to it.
        afterTurn: (created) => onThreadCreated?.(created.id, created.cwd ?? dir),
      });
    } catch (cause) {
      fail(cause);
    } finally {
      session.starting = false;
    }
  }

  const contextStats = $derived(session.tokenUsage ? deriveContextStats(session.tokenUsage) : null);
  const costUsd = $derived(estimateCost(session.tokenUsage, activeModel));

  // A deep link asked for a specific worktree but the opened thread runs
  // elsewhere — surface it rather than silently showing the wrong directory.
  const cwdMismatch = $derived(
    Boolean(expectedCwd && thread?.id && thread?.cwd && !cwdBelongsTo(thread.cwd, expectedCwd)),
  );

  function review(target: ReviewTarget) {
    return session.review(target);
  }

  /** `/diff` — the Changes page, on the working tree. */
  function showWorkingDiff() {
    if (liveThreadId) onOpenChanges?.(liveThreadId, { segment: "uncommitted" });
  }

  /** Commands a draft can answer. `/review` and `/goal` are here deliberately:
   *  both start a thread of their own rather than turning the user away. The
   *  rest read back the conversation, so they need a thread behind them. */
  const DRAFT_COMMANDS = new Set<SlashCommandId>(["new", "review", "goal", "status"]);

  /** Commands answered here rather than in App, given the typed argument. */
  const threadCommands: Partial<Record<SlashCommandId, (argument: string, typed: string) => void>> = {
    btw: (argument) => {
      panelView = { kind: "side", ...(argument ? { submission: { id: Date.now(), text: argument } } : {}) };
    },
    compact: () => void session.compact(),
    review: (argument) => {
      if (activeTurn || starting) {
        session.notice = "/review can't start while Codex is working — stop the current turn first.";
        return;
      }
      // A typed argument is the review instruction itself; with none, ask what to
      // review rather than assuming the working tree.
      if (argument) void review({ type: "custom", instructions: argument });
      else composer?.openReviewPicker();
    },
    diff: showWorkingDiff,
    undo: (argument) => {
      const turns = Number.parseInt(argument, 10);
      void session.undoTurns(Number.isFinite(turns) && turns > 0 ? turns : 1);
    },
    status: () => {
      panelView = { kind: "status" };
    },
    goal: (argument, typed) => void goals.run(argument, typed),
    copy: () => void copyLastResponse(),
    export: () => void exportConversation(),
  };

  /**
   * Thread-scoped commands. Those needing the live thread are answered here;
   * the rest bubble to App. A command that needs a thread and has none tells the
   * user so rather than silently doing nothing.
   */
  function runCommand(command: SlashCommandId, argument = "", typed = "") {
    if (!liveThreadId && !DRAFT_COMMANDS.has(command)) {
      // Say why, and give the typed line back so the user can edit it rather
      // than retype it from scratch.
      session.streamError = `/${command} needs an open thread.`;
      if (typed) composer?.restoreText(typed);
      return;
    }
    const handler = threadCommands[command];
    if (handler) handler(argument, typed);
    else onCommand?.(command, liveThreadId);
  }

  const goals = goalCommands({
    session: () => session,
    restoreText: (text) => composer?.restoreText(text),
  });

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
    const last = [...allItems].reverse().find((item) => item.type === "agentMessage" && !item.streaming && item.text);
    if (!last?.text) {
      session.notice = "No response to copy yet.";
      return;
    }
    try {
      await commands.copyText(last.text);
      session.notice = "Copied the last response as markdown.";
    } catch (cause) {
      fail(cause);
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
      await commands.copyText(sections.join("\n\n"));
      session.notice = "Copied the conversation as markdown.";
    } catch (cause) {
      fail(cause);
    }
  }

  function changeSubagentPolicy(modelPolicy: SubagentPolicy | null, effortPolicy: SubagentPolicy | null) {
    session.subagentModelPolicy = modelPolicy;
    session.subagentReasoningEffortPolicy = effortPolicy;
    if (!liveThreadId) return;
    commands.updateSubagentPolicy(liveThreadId, modelPolicy, effortPolicy).catch(fail);
  }
</script>

<div class="flex h-full">
  <div class="relative flex h-full min-w-0 flex-1 flex-col">
    {#if cwdMismatch}
      <div class="mx-auto w-full max-w-3xl px-6 pt-4">
        <div class="card preset-tonal-warning px-3 py-2 text-xs leading-5">
          This thread's working directory (<span class="font-mono">{thread?.cwd}</span>) does not match the requested
          worktree (<span class="font-mono">{expectedCwd}</span>).
        </div>
      </div>
    {/if}
    <div class="relative flex min-h-0 flex-1 flex-col">
      <!-- Inside the transcript area, not the whole column: anchored to
         `bottom-4` of the column, the open overview sat over the composer and
         swallowed clicks on Stop/Send. -->
      {#if thread && liveThreadId}
        <FloatingMenu
          plan={latestPlan}
          outputs={outputChanges}
          sources={sourceQueries}
          sideQuestionCount={sideQuestions.filter((entry) => entry.parentThreadId === liveThreadId).length}
          subagents={session.subagents}
          processes={processes.list}
          currentThreadId={liveThreadId}
          {contextStats}
          {costUsd}
          onOpenFinder={() => commands.revealInFinder(thread?.cwd || cwd).catch(() => {})}
          onOpenZed={() => commands.openInZed(thread?.cwd || cwd).catch(fail)}
          onShowPlan={() => (panelView = latestPlan ? { kind: "plan", text: latestPlan } : panelView)}
          onShowSources={() => (panelView = { kind: "sources", queries: sourceQueries })}
          onShowSideQuestions={() => (panelView = { kind: "side" })}
          changes={threadChanges}
          onShowChanges={(path) => liveThreadId && onOpenChanges?.(liveThreadId, { path })}
          onShowFiles={() => (panelView = { kind: "files" })}
          onShowMessageLog={() => (panelView = { kind: "messageLog" })}
          onShowStatus={() => (panelView = { kind: "status" })}
          onShowBrowser={browserUse ? () => (panelView = { kind: "browser" }) : undefined}
          onOpenSubagent={(agent) => (onOpenSubagent ? onOpenSubagent(agent) : onSelectThread?.(agent.id))}
          onOpenProcess={(process) => (panelView = { kind: "process", processKey: process.key })}
          onStopProcess={backgroundTerminals ? stopProcess : undefined}
          onStopAllBackground={backgroundTerminals && liveThreadId
            ? () => void stopAllBackground(liveThreadId).catch(fail)
            : undefined}
          onPanelOpen={() => {
            if (liveThreadId && backgroundTerminals) void reconcileBackground(liveThreadId);
          }}
        />
      {/if}
      <div
        class="min-h-0 flex-1 overflow-x-hidden overflow-y-auto"
        {@attach transcript.attach}
        onscroll={transcript.onScroll}
        onwheel={transcript.onWheel}
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
            {#if showSystemPromptInThread && systemPromptParts?.length}
              <SystemPromptSection parts={systemPromptParts} />
            {/if}
            {#each thread.turns as turn, turnIndex (turn.id)}
              {@const parts = splitTurn(turn, thread.harness)}
              {@const collapseDiffs = turnDiffCount(turn) > 1}
              {@const liveSegment = turn.status === "inProgress" ? parts.body.at(-1) : undefined}
              {#each parts.users as item (item.id)}
                <UserMessageBubble
                  {item}
                  cwd={thread?.cwd || cwd}
                  editable={!!liveThreadId &&
                    !turn.id.startsWith("local-") &&
                    !activeTurn &&
                    can(thread?.harness, "revert")}
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
                    subagents={session.subagents}
                  />
                {:else if segment === liveSegment && !segment.collapsed}
                  {@const liveSegments = turnSegments(segment.items)}
                  {#each liveSegments as liveSeg, liveIndex (segmentKey(liveSeg))}
                    {#if liveSeg.kind === "reasoning"}
                      <ReasoningBlock items={liveSeg.items} live={liveIndex === liveSegments.length - 1} />
                    {:else}
                      <WorkItem item={liveSeg.item} {collapseDiffs} subagents={session.subagents} />
                    {/if}
                  {/each}
                {:else}
                  {@const key = workKey(turn.id, segment.items)}
                  <div>
                    <Collapsible
                      class="min-w-0 items-stretch"
                      open={openWork.has(key)}
                      onOpenChange={({ open }) => (open ? openWork.add(key) : openWork.delete(key))}
                    >
                      <Collapsible.Trigger
                        class="group flex items-center gap-1 text-sm text-surface-500 hover:text-surface-700-300"
                      >
                        <span
                          >{segment === firstWork && turn.status !== "inProgress" ? workedLabel(turn) : "Worked"}</span
                        >
                        <ChevronRight size={14} class="transition group-data-[state=open]:rotate-90" />
                      </Collapsible.Trigger>
                      <Collapsible.Content>
                        {#if openWork.has(key)}
                          <div class="mt-3 space-y-4 border-l-2 border-surface-200-800 pl-4">
                            {#each segment.items as item (item.id)}
                              <WorkItem {item} {collapseDiffs} subagents={session.subagents} />
                            {/each}
                          </div>
                        {/if}
                      </Collapsible.Content>
                    </Collapsible>
                    <hr class="mt-3 border-surface-200-800">
                  </div>
                {/if}
              {/each}
              {#if turn.status === "failed" && turn.error}
                {@const steer = turn.error.misalignment?.steer?.message}
                <div class="error-note p-3 space-y-2">
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

            {#each browserApprovals as approval (approval.id)}
              <BrowserApprovalCard {approval} />
            {/each}

            {#each threadQuestions as request (request.requestId)}
              <QuestionCard {request} onAnswered={(item) => thread && upsertItem(thread.turns, request.turnId, item)} />
            {/each}

            {#each threadElicitations as elicitation (elicitation.requestId)}
              <ElicitationCard {elicitation} />
            {/each}

            {#if showTypingIndicator}
              <TypingDots />
            {/if}

            {#if thread.turns.length === 0}
              <p class="py-16 text-center text-sm text-surface-500">
                {threadId ? "This thread has no messages yet." : "Send a message to start a new thread."}
              </p>
            {/if}
          {/if}
        </div>
      </div>
      {#if transcript.showJumpPill && !loading && thread}
        <button
          type="button"
          class="btn btn-sm preset-filled absolute bottom-3 left-1/2 -translate-x-1/2 gap-1 rounded-full shadow-lg"
          onclick={() => transcript.follow(true)}
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

    {#if session.goal}
      {@const goal = session.goal}
      <GoalBanner
        {goal}
        onEditBudget={goals.editBudget}
        onEdit={() => composer?.startGoalEdit(goal.objective)}
        onToggle={goals.toggle}
        onClear={goals.clear}
      />
    {/if}

    {#if session.notice}
      <div class="mx-auto w-full max-w-3xl px-6">
        <div class="card preset-tonal-warning mb-2 flex items-start gap-2 px-3 py-2 text-xs">
          <span class="min-w-0 flex-1">{session.notice}</span>
          <button
            type="button"
            onclick={() => (session.notice = null)}
            class="shrink-0 opacity-60 hover:opacity-100"
            aria-label="Dismiss notice"
          >
            <X size={12} />
          </button>
        </div>
      </div>
    {/if}

    {#if session.pendingSteers.length > 0 || session.queue.entries.length > 0}
      <div class="mx-auto w-full max-w-3xl space-y-1 px-6 pb-2">
        {#each session.pendingSteers as steer (steer.clientId)}
          <div
            class="flex items-start gap-2 rounded-lg border border-primary-500/30 bg-primary-500/10 px-3 py-1.5 text-xs text-surface-600-400"
            title="Sent into the running turn; Codex reads it at its next step."
          >
            <span class="shrink-0 pt-0.5 font-medium text-primary-600-400">Steering</span>
            <span class="min-w-0 flex-1 truncate pt-0.5">{queuedText(steer.input)}</span>
          </div>
        {/each}
        {#each session.queue.entries as entry, index (entry.id)}
          <QueuedMessageRow
            {entry}
            canSendNow={activeTurn !== null || index > 0}
            onSendNow={() => session.queue.promote(entry)}
            onSteer={session.canSteer ? () => session.queue.steer(entry) : undefined}
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
            <Segmented
              label="Start in"
              value={draft.location}
              options={newThreadLocations}
              onChange={chooseNewThreadLocation}
            />
            {#if draft.location === "permanent"}
              <span
                class="min-w-0 flex-1 truncate px-2 text-right font-mono text-[10px] text-surface-500"
                title={draft.worktreePath}
              >
                {draft.worktreePath}
              </span>
            {/if}
          {/if}
        </div>
      </div>
    {/if}

    <Composer
      bind:this={composer}
      busy={activeTurn !== null || starting || session.compacting}
      disabled={loading || error !== null}
      hasQuestions={threadQuestions.length > 0}
      plan={pendingPlan}
      cwd={workDir}
      draftKey={threadId ? `${projectPath || cwd}#thread:${threadId}` : projectPath || cwd}
      history={messageHistory}
      projectKey={projectPath || cwd}
      {threadId}
      worktreeContext={draft.worktreeContext}
      threadHarness={harnessKind(thread?.harness) ?? (threadId ? "codex" : null)}
      canSteer={session.canSteer}
      onSend={send}
      onSteer={steer}
      onInterrupt={() => session.interrupt()}
      onCommand={runCommand}
      onGoal={goals.set}
      onReview={(target) => void review(target)}
      onImplementFresh={liveThreadId ? startPlanThread : undefined}
      {contextStats}
      compacting={session.compacting}
      subagentModelPolicy={session.subagentModelPolicy}
      subagentReasoningEffortPolicy={session.subagentReasoningEffortPolicy}
      threadModel={lastTurnModel}
      threadSpeedTier={thread?.speedTier}
      runningSpeedTier={session.runningSpeedTier}
      onSubagentPolicyChange={changeSubagentPolicy}
      onModelChange={(modelId) => (activeModel = modelId)}
      onDraftChoices={(choices) => {
        draft.harness = choices.harness;
        draft.includeWorktreeContext = choices.worktreeContext;
      }}
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
      {contextStats}
      {costUsd}
      {activeModel}
      cwd={workDir}
      onClose={() => (panelView = null)}
      onDataChanged={(data) => onDataChanged?.(data)}
      onImplementPlan={implementPlan}
      onImplementPlanFresh={liveThreadId ? implementPlanFresh : undefined}
      implementDisabled={activeTurn !== null || starting}
      {showSystemPromptInThread}
      onToggleSystemPromptInThread={toggleSystemPromptInThread}
      onStopProcess={backgroundTerminals ? stopProcess : undefined}
      onStopProcessTurn={(process) => {
        if (process.turnId && process.threadId) {
          commands.interruptTurn(process.threadId, process.turnId).catch(fail);
        }
      }}
    />
  {/if}
</div>
