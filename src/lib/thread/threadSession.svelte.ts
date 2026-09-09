/**
 * One thread's live state — its transcript, queue, goal, usage and stream
 * status — kept for as long as the thread has work in flight, whether or not
 * a view is showing it.
 *
 * `ThreadView` renders one of these and turns user intent into calls on it;
 * `sessions.svelte.ts` keeps them alive and routes Codex events in. There is
 * one event handler per thread, so what happens to a thread when an event
 * arrives is decided in exactly one place regardless of what is on screen.
 */
import { setAgentRuns } from "$lib/services/agentRuns.svelte";
import {
  compactThread,
  getThreadGoal,
  interruptTurn,
  invalidateThreadCache,
  isRevertUnsupported,
  isTurnSettingsUnsupported,
  listAgentRuns,
  listSubagents,
  readThread,
  revertThread,
  rollbackThread,
  startReview,
  startTurn,
  updateTurnSettings,
} from "$lib/services/api";
import { activeTurns, type CodexEvent, threadTokenUsage } from "$lib/services/codexEvents.svelte";
import { requestAutoName } from "$lib/thread/autoName";
import { ThreadQueue } from "$lib/thread/threadQueue.svelte";
import { applyThreadEvent, BUFFERING_NOTICE, ensureTurn, finalizeRunningTurns } from "$lib/thread/threadStream";
import { toastError } from "$lib/toaster";
import type {
  ReviewTarget,
  SubagentDetail,
  SubagentPolicy,
  ThreadDetail,
  ThreadGoal,
  ThreadTokenUsage,
  Turn,
  TurnOptions,
  UserInputPart,
} from "$lib/types";

/** Notices for a model/effort change made mid-turn. */
export const LIVE_SETTINGS_APPLIED = "Switched the running turn to the new settings.";
export const LIVE_SETTINGS_NEXT_TURN = "New settings apply from the next turn.";

export class ThreadSession {
  /** Codex's id for the thread; `null` while it is an unsent draft. */
  id = $state<string | null>(null);
  /** The transcript. Events mutate it in place; the proxy makes that reactive. */
  thread = $state<ThreadDetail | null>(null);
  runningSpeedTier = $state<string | null | undefined>(undefined);
  private settingsUpdate: Promise<void> = Promise.resolve();
  loading = $state(false);
  /** A fatal load error: there is no transcript to show. */
  error = $state<string | null>(null);
  /** A thread-level operation in flight (create, revert, review) that must
   *  finish before another turn can start. Legacy draft and fork callers still toggle this around theirs. */
  starting = $state(false);
  compacting = $state(false);
  /** The turn Codex runs the compaction under, once its `turn/started` lands. */
  #compactionTurnId: string | null = null;
  /** The mode the harness last said the thread is in, when it changed the
   *  mode itself (`thread/collaborationMode/changed`); null until then. */
  collaborationMode = $state<"plan" | "default" | null>(null);
  /** What a send with no options of its own runs with — the composer's
   *  current choices. Set by the view; without it a stranded answer, a steer
   *  or a drain that lost its options would go out mode-less, and the harness
   *  would keep (Codex) or drop (Claude) plan mode regardless of the pill. */
  turnOptions: (() => TurnOptions | undefined) | null = null;
  /** An error that ended a turn or the stream. The view shows it as a toast and
   *  clears it; while it stands the session is retained as "working". */
  streamError = $state<string | null>(null);
  /**
   * The last advisory Codex sent — a warning, a model reroute, a retryable
   * error. Deliberately separate from `streamError`: none of these ended the
   * turn, and showing them in the error card would make a thread that
   * recovered look broken. Cleared when the next turn starts.
   */
  notice = $state<string | null>(null);
  /** The goal this thread is working towards, kept live from
   *  `thread/goal/updated` so it reflects what Codex holds. */
  goal = $state<ThreadGoal | null>(null);
  tokenUsage = $state<ThreadTokenUsage | null>(null);
  subagentModelPolicy = $state<SubagentPolicy | null>(null);
  subagentReasoningEffortPolicy = $state<SubagentPolicy | null>(null);
  /** Codex-spawned subagents (`listSubagents`); app-run agents come from the
   *  agent-runs store and are merged by the view. */
  subagents = $state<SubagentDetail[]>([]);
  /** Bumped on every applied stream event, so a view can follow the transcript
   *  (scroll) without owning the handler. */
  revision = $state(0);
  /** Views currently showing this session. Retention looks at it. */
  mounted = 0;
  readonly queue: ThreadQueue;
  /** Called after work ends with no view attached; the registry drops the
   *  session then. */
  onIdle: (() => void) | null = null;

  /** The in-flight `startTurn` call, so a Stop pressed before it resolves can
   *  wait for the real turn id instead of silently doing nothing. */
  private pendingTurnStart: Promise<unknown> | null = null;
  private disposed = false;
  private disconnectedState = false;
  private operation: { kind: "review" | "compact" | "undo" } | null = null;
  private pendingReview: Promise<Turn | null> | null = null;

  private available(): boolean {
    return (
      !this.disposed &&
      !this.disconnectedState &&
      !this.loading &&
      !this.starting &&
      !this.compacting &&
      !this.operation &&
      !this.pendingTurnStart &&
      !this.activeTurn
    );
  }

  private finishOperation(operation: NonNullable<ThreadSession["operation"]>): void {
    if (this.operation !== operation) return;
    this.operation = null;
    this.starting = false;
    if (this.disposed || this.disconnectedState) return;
    this.queue.maybeDrain();
    if (this.mounted === 0 && !this.working()) this.onIdle?.();
  }

  /** Own the request even while a draft is being created by the view. */
  async review(target: ReviewTarget, ensureLiveThread?: () => Promise<string>): Promise<void> {
    if (!this.available() || !this.thread || (!this.id && !ensureLiveThread)) return;
    const operation = { kind: "review" } as const;
    this.operation = operation;
    this.starting = true;
    this.streamError = null;
    const request = async (): Promise<Turn | null> => {
      try {
        const id = this.id ?? (await ensureLiveThread!());
        if (this.operation !== operation || this.disposed) return null;
        const turn = await startReview(id, target);
        if (this.operation !== operation || this.disposed) return null;
        const existing = this.thread!.turns.find((candidate) => candidate.id === turn.id);
        if (!existing)
          this.thread!.turns.push({ ...turn, status: turn.status ?? "inProgress", items: turn.items ?? [] });
        this.revision++;
        return turn;
      } catch (cause) {
        if (this.operation === operation && !this.disposed) {
          this.streamError = cause instanceof Error ? cause.message : String(cause);
        }
        return null;
      }
    };
    const pending = request();
    this.pendingReview = pending;
    await pending;
    if (this.pendingReview === pending) this.pendingReview = null;
    this.finishOperation(operation);
  }

  /** Request acknowledgement does not end the streamed compaction. */
  async compact(): Promise<void> {
    if (!this.available() || !this.id) return;
    const operation = { kind: "compact" } as const;
    this.operation = operation;
    this.starting = true;
    this.beginCompaction();
    this.streamError = null;
    try {
      await compactThread(this.id);
    } catch (cause) {
      if (this.operation === operation && !this.disposed) {
        this.endCompaction();
        this.streamError = cause instanceof Error ? cause.message : String(cause);
      }
    } finally {
      this.finishOperation(operation);
    }
  }

  /** Rewind history in place, falling back only when revert is unsupported. */
  async undoTurns(count: number): Promise<void> {
    if (!this.available() || !this.id || !this.thread || !Number.isInteger(count) || count <= 0) return;
    const thread = this.thread;
    const id = this.id;
    const dropped = Math.min(count, thread.turns.length);
    if (!dropped) return;
    const kept = thread.turns.slice(0, -dropped);
    const operation = { kind: "undo" } as const;
    this.operation = operation;
    this.starting = true;
    this.streamError = null;
    try {
      try {
        await revertThread(
          id,
          thread.turns[kept.length].id,
          kept.map((turn) => turn.id),
        );
      } catch (cause) {
        if (this.operation !== operation || this.disposed) return;
        if (!isRevertUnsupported(cause)) throw cause;
        await rollbackThread(id, dropped);
      }
      if (this.operation !== operation || this.disposed) return;
      thread.turns = kept;
      this.revision++;
      await invalidateThreadCache(id).catch(() => {});
    } catch (cause) {
      if (this.operation === operation && !this.disposed) {
        this.streamError = cause instanceof Error ? cause.message : String(cause);
      }
    } finally {
      this.finishOperation(operation);
    }
  }
  /** Bumped by every deliberate goal write, so the async fetch in `load()`
   *  can tell when its result has been overtaken and must be dropped. */
  #goalEpoch = 0;

  /** Set the goal. All writes go through here so they invalidate any
   *  still-in-flight `getThreadGoal` from `load()`. */
  setGoal(goal: ThreadGoal | null): void {
    this.#goalEpoch++;
    this.goal = goal;
  }

  constructor(id: string | null, draftCwd?: string) {
    this.id = id;
    if (!id) this.thread = { id: "", preview: "", cwd: draftCwd ?? "", turns: [] };
    this.queue = new ThreadQueue({
      threadId: () => this.id,
      send: (input, options) => this.send(input, options),
      interrupt: () => this.interrupt(),
      idle: () => this.available(),
      onNotice: (text) => {
        this.notice = text;
      },
      onError: toastError,
    });
  }

  get activeTurn() {
    return this.thread?.turns.find((candidate) => candidate.status === "inProgress") ?? null;
  }

  /** Whether the session is worth keeping when nothing is showing it. */
  working(): boolean {
    const id = this.id;
    return (
      this.operation !== null ||
      this.starting ||
      this.compacting ||
      this.pendingTurnStart !== null ||
      (id !== null && activeTurns.list.includes(id)) ||
      this.activeTurn !== null ||
      this.queue.entries.length > 0 ||
      this.queue.draining ||
      this.streamError !== null
    );
  }

  /** Read the transcript from Codex. Only for sessions created with an id. */
  async load(): Promise<void> {
    const id = this.id;
    if (!id) return;
    this.disconnectedState = false;
    this.loading = true;
    this.error = null;
    // Codex replays `thread/tokenUsage/updated` on a thread's first resume; after
    // that the cached figure is all we have until the next turn reports one.
    this.tokenUsage = threadTokenUsage[id] ?? null;
    // A goal set locally (e.g. `/goal` while this fetch is in flight) must not
    // be clobbered by a late resolve: apply only if nothing wrote since.
    const goalEpoch = ++this.#goalEpoch;
    void getThreadGoal(id)
      .then((current) => {
        if (!this.disposed && this.#goalEpoch === goalEpoch) this.goal = current;
      })
      .catch(() => {});
    try {
      // The cached detail is keyed by the summary's `updated_at`, which does not
      // move while a turn runs — for a working thread it is stale by construction.
      if (activeTurns.list.includes(id)) await invalidateThreadCache(id).catch(() => {});
      const detail = await readThread(id);
      if (this.disposed) return;
      // A turn left `inProgress` by a session that has since died would render
      // as working forever — nothing can complete it, so show it as what it is.
      if (!activeTurns.list.includes(id)) finalizeRunningTurns(detail.turns, "interrupted");
      this.thread = detail;
      this.runningSpeedTier = detail.speedTier;
      this.subagentModelPolicy = detail.subagentModelPolicy ?? null;
      this.subagentReasoningEffortPolicy = detail.subagentReasoningEffortPolicy ?? null;
      void this.refreshSubagents();
      // Messages queued by an earlier session (or another client) are durable
      // on the server; pick them up so the drain can run them.
      this.queue.syncFromServer();
    } catch (cause) {
      if (this.disposed) return;
      this.error = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (!this.disposed) this.loading = false;
    }
    this.queue.maybeDrain();
  }

  /** The draft became a real thread: run under its id from here on. */
  attach(id: string): boolean {
    if (this.disposed || this.disconnectedState) return false;
    this.id = id;
    if (this.thread) this.thread.id = id;
    return true;
  }

  /** The registry is letting go of this session; late I/O must not touch it. */
  dispose(): void {
    this.disposed = true;
    this.operation = null;
    this.pendingReview = null;
  }

  /**
   * Start a turn, or queue the message when Codex is busy. Returns whether the
   * message is accounted for — started as a turn or safely queued. `false`
   * means it reached nothing, so a caller holding the only copy (the queue's
   * drain) must put it back.
   *
   * A draft has to be created by the view first (that needs the composer's
   * choices); `attach` it, then call this.
   */
  async send(input: UserInputPart[], options?: TurnOptions): Promise<boolean> {
    const thread = this.thread;
    if (!thread || this.disposed) return false;
    // An explicit send is a retry; late operation callbacks remain invalidated.
    this.disconnectedState = false;
    options ??= this.turnOptions?.();
    // A compaction is a turn too, even before its `turn/started` arrives: a
    // send now would race it, and its start would adopt the optimistic bubble.
    if (this.activeTurn || this.starting || this.compacting || this.operation || this.pendingTurnStart) {
      // Codex is mid-turn: park the message and send it once the turn ends
      // (completed or interrupted via Stop/Esc).
      void this.queue.add(input, options);
      return true;
    }
    const id = this.id;
    if (!id) return false;
    this.streamError = null;
    // Sending again is the retry for a drain that failed, so let the queue move.
    this.queue.unblock();
    const localTurnId = `local-${Date.now()}`;
    try {
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
      this.revision++;
      const start = startTurn(id, input, options);
      this.pendingTurnStart = start;
      const turn = await start;
      if (options?.speedTier !== undefined) thread.speedTier = options.speedTier;
      this.runningSpeedTier = thread.speedTier;
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
      this.streamError = cause instanceof Error ? cause.message : String(cause);
      return false;
    } finally {
      this.pendingTurnStart = null;
      if (!this.disposed && !this.disconnectedState) this.queue.maybeDrain();
    }
  }

  async interrupt(): Promise<void> {
    if (this.disposed || this.disconnectedState) return;
    const review = this.pendingReview;
    if (review) {
      const turn = await review;
      if (!turn || this.disposed || this.disconnectedState || this.activeTurn?.id !== turn.id || !this.id) return;
      try {
        await interruptTurn(this.id, turn.id);
      } catch (cause) {
        if (!this.disposed && !this.disconnectedState)
          this.streamError = cause instanceof Error ? cause.message : String(cause);
      }
      return;
    }
    const id = this.id;
    let active = this.activeTurn;
    if (!id || !active) return;
    if (active.id.startsWith("local-")) {
      // The optimistic turn is still waiting on `turn/start`; Codex has never
      // heard of it. Wait for the real id, then interrupt that.
      try {
        await this.pendingTurnStart;
      } catch {
        return; // send() already surfaced the error and removed the turn.
      }
      active = this.activeTurn;
      if (!this.id || !active || active.id.startsWith("local-")) return;
    }
    interruptTurn(id, active.id).catch((cause) => {
      this.streamError = cause instanceof Error ? cause.message : String(cause);
    });
  }

  /**
   * The user changed model or effort while a turn is running. Newer Codex can
   * switch the running turn over (`turn/settings/update`); everything else —
   * an older Codex, or a turn past the point of switching — keeps today's
   * behaviour, where the choice applies from the next turn. Never throws: the
   * preference itself is already saved by the composer.
   */
  updateLiveSettings(settings: { model?: string | null; effort?: string | null; speedTier?: string }): Promise<void> {
    const id = this.id;
    const active = this.activeTurn;
    const nextTurn = settings.speedTier !== undefined ? "Fast mode will change next turn." : LIVE_SETTINGS_NEXT_TURN;
    this.settingsUpdate = this.settingsUpdate.then(async () => {
      if (this.disposed) return;
      if (!id || !active || active.id.startsWith("local-") || this.activeTurn?.id !== active.id) {
        this.notice = nextTurn;
        return;
      }
      try {
        const status = await updateTurnSettings(id, active.id, settings);
        if (this.disposed) return;
        const applied = status === "applied" && this.activeTurn?.id === active.id;
        if (applied && settings.speedTier !== undefined) this.runningSpeedTier = settings.speedTier;
        this.notice = applied ? LIVE_SETTINGS_APPLIED : nextTurn;
      } catch (cause) {
        if (isTurnSettingsUnsupported(cause)) this.notice = nextTurn;
        else toastError(cause instanceof Error ? cause.message : String(cause));
      }
    });
    return this.settingsUpdate;
  }

  // Deliberately two independent requests rather than one `Promise.all`:
  // `listSubagents` resumes and re-reads every descendant thread, so it is slow
  // and can stall. Gating our own runs behind it would leave the transcript's
  // agent cards inert and the menu empty for as long as Codex takes — or forever.
  async refreshSubagents(): Promise<void> {
    const id = this.id;
    if (!id) return;
    void (async () => {
      try {
        setAgentRuns(id, await listAgentRuns(id));
      } catch {
        // Leave whatever the store already had rather than blanking the menu.
      }
    })();
    try {
      const codex = await listSubagents(id);
      if (id === this.id) this.subagents = codex;
    } catch {
      if (id === this.id) this.subagents = [];
    }
  }

  /** A subagent of this thread changed status. */
  setSubagentStatus(subagentId: string, status: unknown): void {
    const target = this.subagents.find((candidate) => candidate.id === subagentId);
    if (!target) return;
    const next = (status as { type?: string } | null)?.type ?? status;
    if (typeof next === "string") target.status = next;
  }

  /** Begin a compaction: the session is busy until it lands or fails. */
  private beginCompaction(): void {
    this.compacting = true;
    this.#compactionTurnId = null;
  }

  private endCompaction(): void {
    this.compacting = false;
    this.#compactionTurnId = null;
  }

  /** The stream disconnected: nothing in flight can finish. */
  disconnected(): void {
    this.disconnectedState = true;
    this.operation = null;
    this.pendingReview = null;
    this.starting = false;
    this.streamError = "Lost connection to Codex.";
    this.endCompaction();
    finalizeRunningTurns(this.thread?.turns ?? [], "interrupted");
    this.revision++;
  }

  /** Apply an event addressed to this thread (`params.threadId === id`). */
  handleEvent(event: CodexEvent): void {
    if (this.disposed) return;
    const { method, params } = event;
    if (method === "turn/started") this.disconnectedState = false;
    if (this.disconnectedState) return;
    const id = this.id;
    if (!id || !params) return;
    if (method === "thread/goal/updated") {
      // Updates without a goal payload (usage/turn bookkeeping) must not blank
      // the goal; only `thread/goal/cleared` removes it.
      if (params.goal) this.setGoal(params.goal);
      return;
    }
    if (method === "thread/goal/cleared") {
      this.setGoal(null);
      return;
    }
    if (method === "thread/tokenUsage/updated") {
      this.tokenUsage = params.tokenUsage ?? null;
      return;
    }
    if (method === "thread/compacted") {
      this.endCompaction();
      this.queue.maybeDrain();
      if (this.mounted === 0 && !this.working()) this.onIdle?.();
      return;
    }
    if (method === "thread/collaborationMode/changed") {
      this.collaborationMode = params.mode === "plan" ? "plan" : "default";
      return;
    }
    if (method === "thread/queue/changed") {
      this.queue.syncFromServer();
      return;
    }
    if (method === "thread/reverted") {
      // Another client truncated this thread's history; the local transcript and
      // cache are both stale. Force a re-read on next load. Not while we are the
      // ones reverting — that path manages the transcript itself.
      if (!this.starting) invalidateThreadCache(id).catch(() => {});
      return;
    }
    if (method === "thread/settings/updated") {
      this.subagentModelPolicy = params.threadSettings?.subagentModelPolicy ?? null;
      this.subagentReasoningEffortPolicy = params.threadSettings?.subagentReasoningEffortPolicy ?? null;
      return;
    }
    const thread = this.thread;
    if (!thread) return;
    if (method === "turn/started") {
      this.notice = null;
      // The first turn to start while compacting is the compaction itself.
      if (this.compacting && !this.#compactionTurnId) this.#compactionTurnId = turnIdOf(params);
    }
    // Review can complete before its request returns, without a start or item.
    if (this.operation?.kind === "review" && method === "turn/completed") {
      const completedId = turnIdOf(params);
      if (completedId) ensureTurn(thread.turns, completedId);
    }
    const outcome = applyThreadEvent(thread, event);
    if (outcome.streamError) this.streamError = outcome.streamError;
    if (outcome.notice) this.notice = outcome.notice;
    // The buffering notice describes an ongoing stall; once the stall ends — or
    // the turn does — it would just read as a hang, so take it down.
    if ((outcome.bufferingEnded || outcome.turnCompleted) && this.notice === BUFFERING_NOTICE) this.notice = null;
    if (outcome.turnCompleted) {
      // Compaction runs as a turn, so its end — however it ends — releases the
      // meter. Only its own end, though: a turn that raced it must not.
      if (this.compacting && this.#compactionTurnId !== null && this.#compactionTurnId === turnIdOf(params)) {
        this.endCompaction();
      }
      // The detail cache still holds the transcript from before this turn; drop
      // it so a later read of this thread does not serve it back.
      invalidateThreadCache(id).catch(() => {});
      // The end of the opening turn is the first moment a title can reflect what
      // the thread actually turned out to be about, so re-name off the exchange.
      if (thread.turns.length === 1) requestAutoName(id, "reply");
    }
    if (outcome.subagentsChanged) void this.refreshSubagents();
    this.revision++;
    if (outcome.turnCompleted) {
      this.queue.maybeDrain();
      if (this.mounted === 0 && !this.working()) this.onIdle?.();
    }
  }
}

/** The turn a `turn/started` or `turn/completed` names. */
function turnIdOf(params: unknown): string | null {
  const turn = (params as { turn?: { id?: unknown } }).turn;
  return typeof turn?.id === "string" ? turn.id : null;
}
