/**
 * The messages waiting to run on one thread, and the one rule about them: a
 * message is never lost. It sits on the server queue when Codex can hold it,
 * in this window when it cannot, and between being taken off the queue and
 * handed to `send` this object holds the only copy.
 *
 * Everything the invariant needs lives here — the optimistic entries, the
 * options the server queue has no field for, the in-flight counter that keeps
 * a re-list from clobbering an optimistic write, and the drain policy. The
 * owner supplies `send`, `interrupt` and `idle`; nothing here knows about
 * components.
 */
import { isQueueUnsupported, queueAdd, queueDelete, queueList, queueReorder, queueUpdate } from "$lib/services/api";
import { isClientQueued, localId, mergeQueue, pendingId } from "$lib/thread/queueEntries";
import type { QueuedSubmission, TurnOptions, UserInputPart } from "$lib/types";

export interface QueueDeps {
  /** Thread the queue belongs to; `null` while it is still a draft, in which
   *  case nothing reaches the server and entries drain once it exists. */
  threadId: () => string | null;
  /** Start a turn. Resolves `false` when the message reached nothing, in which
   *  case the queue puts it back rather than dropping it. */
  send: (input: UserInputPart[], options?: TurnOptions) => Promise<boolean>;
  /** Stop the running turn, so a promoted message goes next. */
  interrupt: () => Promise<void>;
  /** Whether the thread can take a turn right now — nothing running, nothing
   *  starting, nothing still loading. */
  idle: () => boolean;
  /** A non-fatal word about a message that stayed local unexpectedly. */
  onNotice: (text: string) => void;
  /** A failure the user should hear about but that changes nothing. */
  onError: (text: string) => void;
}

export class ThreadQueue {
  /** Mirror of the server queue plus the entries only this window holds. */
  entries = $state<QueuedSubmission[]>([]);
  /** Per-message turn options — the server queue has no field for them, so
   *  they live only in this session, keyed by `clientUserMessageId`. */
  private options = new Map<string, TurnOptions>();
  /** Local mutations in flight; while > 0, server re-lists are skipped so they
   *  cannot clobber an optimistic entry. */
  private mutations = 0;
  private inFlight = false;
  /** Set when a drain put its message back: nothing else would change, so
   *  without this the retry fires again immediately and spins. Cleared by the
   *  next send or promote, which are also the retry. */
  private drainBlocked = false;

  constructor(private readonly deps: QueueDeps) {}

  /** Park a message, rendering it optimistically.
   *
   *  A failure here never drops the message: the entry stays as a local-only
   *  one and still drains when the turn finishes. The server queue is durable
   *  and visible to other clients where the local one is not, so the loss is
   *  persistence, not the message. */
  async add(input: UserInputPart[], options?: TurnOptions): Promise<void> {
    const threadId = this.deps.threadId();
    const clientUserMessageId = crypto.randomUUID();
    if (options) this.options.set(clientUserMessageId, options);
    // Draft thread: there is nothing to queue against yet, so it is local from
    // the start and drains once the thread exists.
    const id = threadId ? pendingId(clientUserMessageId) : localId(clientUserMessageId);
    const optimistic: QueuedSubmission = { id, input, clientUserMessageId };
    this.entries.push(optimistic);
    if (!threadId) {
      this.maybeDrain();
      return;
    }
    this.mutations++;
    try {
      const submission = await queueAdd(threadId, input, clientUserMessageId);
      this.replace(optimistic.id, submission);
    } catch (cause) {
      this.replace(optimistic.id, { ...optimistic, id: localId(clientUserMessageId) });
      // An unsupported queue is a property of this Codex, not something that
      // went wrong — the chip says "Queued locally" and that is the whole story.
      // Other failures (a full queue, a lost thread) are worth a word, but as a
      // notice: nothing ended the turn, and the message is still going to send.
      if (!isQueueUnsupported(cause)) {
        const message = cause instanceof Error ? cause.message : String(cause);
        this.deps.onNotice(
          `Queued in this window only — Codex could not hold it (${message}). It will send when this turn finishes.`,
        );
      }
    } finally {
      this.mutations--;
    }
    this.maybeDrain();
  }

  /** Take a message off the queue, server-side too so it cannot resurrect. */
  async remove(entry: QueuedSubmission): Promise<void> {
    this.entries = this.entries.filter((candidate) => candidate.id !== entry.id);
    this.options.delete(entry.clientUserMessageId);
    const threadId = this.deps.threadId();
    if (!threadId || isClientQueued(entry)) return;
    this.mutations++;
    try {
      await queueDelete(threadId, entry.id);
    } catch {
      // Already gone (started or deleted elsewhere) — the re-list will settle it.
    } finally {
      this.mutations--;
    }
  }

  /** Replace a message's content, on the server too when it lives there. */
  async edit(entry: QueuedSubmission, input: UserInputPart[]): Promise<void> {
    this.entries = this.entries.map((candidate) => (candidate.id === entry.id ? { ...candidate, input } : candidate));
    const threadId = this.deps.threadId();
    if (!threadId || isClientQueued(entry)) return;
    this.mutations++;
    try {
      await queueUpdate(threadId, entry.id, input);
    } catch (cause) {
      this.deps.onError(
        `Could not update the queued message: ${cause instanceof Error ? cause.message : String(cause)}`,
      );
    } finally {
      this.mutations--;
    }
  }

  /** Jump a message to the head and stop the running turn so it goes next. */
  async promote(entry: QueuedSubmission): Promise<void> {
    this.entries = [entry, ...this.entries.filter((candidate) => candidate.id !== entry.id)];
    const threadId = this.deps.threadId();
    if (threadId && !this.entries.some(isClientQueued) && this.entries.length > 1) {
      this.mutations++;
      try {
        await queueReorder(
          threadId,
          this.entries.map((candidate) => candidate.id),
        );
      } catch {
        // The local order still drives the drain; the server's is only cosmetic.
      } finally {
        this.mutations--;
      }
    }
    this.drainBlocked = false;
    await this.deps.interrupt();
    this.maybeDrain();
  }

  /** Re-mirror the server queue, unless our own mutation is still in flight. */
  syncFromServer(): void {
    const threadId = this.deps.threadId();
    if (!threadId || this.mutations > 0) return;
    queueList(threadId)
      .then((items) => {
        if (threadId !== this.deps.threadId() || this.mutations > 0) return;
        // Keeps the client-only entries the server does not know about — both
        // the ones still in flight and the ones it will never hold.
        this.entries = mergeQueue(items, this.entries);
        this.maybeDrain();
      })
      .catch(() => {});
  }

  /** A send from the user is the retry for a drain that failed. */
  unblock(): void {
    this.drainBlocked = false;
  }

  /** Run the head of the queue if the thread is free to take it. Called at
   *  every transition that could make it so; a no-op otherwise. */
  maybeDrain(): void {
    if (this.inFlight || this.drainBlocked || !this.deps.idle()) return;
    const next = this.entries[0];
    if (next) void this.drain(next);
  }

  /** Run the head of the queue: take it off the server, then send it through
   *  the normal turn path so its options (which the server queue cannot hold)
   *  and the optimistic bubble behave exactly like a direct send.
   *
   *  Between the removal and the send this holds the only copy of the message,
   *  so a failed send puts it back rather than dropping it. */
  private async drain(next: QueuedSubmission): Promise<void> {
    this.inFlight = true;
    const options = this.options.get(next.clientUserMessageId);
    try {
      await this.remove(next);
      this.options.delete(next.clientUserMessageId);
      if (!(await this.deps.send(next.input, options))) {
        if (options) this.options.set(next.clientUserMessageId, options);
        this.entries = [{ ...next, id: localId(next.clientUserMessageId) }, ...this.entries];
        this.drainBlocked = true;
      }
    } finally {
      this.inFlight = false;
    }
    this.maybeDrain();
  }

  /** A message is between the queue and `send` right now — the only copy of it
   *  is in flight, so its owner must not be dropped. */
  get draining(): boolean {
    return this.inFlight;
  }

  private replace(id: string, entry: QueuedSubmission): void {
    const index = this.entries.findIndex((candidate) => candidate.id === id);
    if (index >= 0) this.entries[index] = entry;
  }
}
