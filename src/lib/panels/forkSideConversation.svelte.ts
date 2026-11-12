/**
 * The fork adapter of a side question's conversation: each side question is a
 * hidden fork of the parent thread, carrying its history so the model has
 * full context, and its questions run there as ordinary turns.
 *
 * The fork's transcript, stream and turn lifecycle are a `ThreadSession` from
 * the session registry, so it is read, streamed and stopped exactly as a
 * thread view's is. The session is keyed by the fork's id, never the parent's,
 * and is released when the panel leaves it: kept while its answer is still
 * coming, dropped once it is idle.
 */

import type { SideConversation, SideConversationHost, SideTranscript } from "$lib/panels/sideConversation.svelte";
import { forkThread } from "$lib/services/api";
import { commands } from "$lib/services/homeRouting";
import { openSession, releaseSession } from "$lib/thread/sessions.svelte";
import type { ThreadSession } from "$lib/thread/threadSession.svelte";
import { finalizeRunningTurns } from "$lib/thread/threadStream";
import type { SideQuestion, UserInputPart } from "$lib/types";
import { errorMessage } from "$lib/utils/errors";

/** How long Stop waits for Codex to confirm before the turn is ended here.
 *  Codex answers "no active turn" (no event follows) for a turn that already
 *  died; without this the composer would stay locked on it. */
export const STOP_GRACE_MS = 1500;

const textInput = (text: string): UserInputPart[] => [{ type: "text", text }];

export class ForkSideConversation implements SideConversation {
  readonly about = {
    text: "Ask a question about this thread without adding it to the main conversation. The side question runs on a fork with full context.",
    pinned: false,
  };
  openId = $state<string | null>(null);
  #session = $state.raw<ThreadSession | null>(null);
  /** Forking a new side question, before its session can take the turn. */
  #forking = $state(false);
  #error = $state<string | null>(null);
  #stopTimer: ReturnType<typeof setTimeout> | undefined;
  #disposed = false;
  readonly #parent: string;
  readonly #host: SideConversationHost;

  constructor(parentThreadId: string, host: SideConversationHost) {
    this.#parent = parentThreadId;
    this.#host = host;
  }

  get entries(): SideQuestion[] {
    return this.#host.sideQuestions().filter((entry) => entry.parentThreadId === this.#parent);
  }

  get transcript(): SideTranscript | null {
    const session = this.#session;
    const thread = session?.thread;
    if (!session || !thread) return null;
    return {
      shape: "turns",
      thread,
      turns: thread.turns.slice(session.inheritedTurns),
      activeTurn: session.activeTurn,
    };
  }

  get loading(): boolean {
    return this.#session?.loading ?? false;
  }

  get busy(): boolean {
    return this.#forking || (this.#session?.activeTurn ?? null) !== null;
  }

  get error(): string | null {
    return this.#error ?? this.#session?.streamError ?? this.#session?.error ?? null;
  }

  open(id: string | null): void {
    if (this.#disposed || id === this.openId) return;
    this.#letGo();
    this.#error = null;
    this.openId = id;
    if (!id) return;
    const session = openSession(id);
    // Questions recorded before the fork point was tracked have no count and
    // show the whole fork.
    session.inheritedTurns = this.entries.find((entry) => entry.sideThreadId === id)?.inheritedTurns ?? 0;
    this.#session = session;
  }

  async ask(text: string): Promise<boolean> {
    if (this.#disposed || this.busy || this.loading) return false;
    this.#error = null;
    if (this.#session) return this.#session.send(textInput(text));
    this.#forking = true;
    try {
      const session = await this.#fork(text);
      if (!session) return this.#disposed;
      // `send` puts the question in the transcript before it awaits, so the
      // composer stays busy from the fork straight into the turn.
      const sent = session.send(textInput(text));
      this.#forking = false;
      return await sent;
    } finally {
      this.#forking = false;
    }
  }

  /** Fork the parent and file the fork as a side question. Returns its
   *  session, now the open one, or `null` when there is nothing to send on
   *  here: the fork failed, or the panel moved on and the question was sent
   *  in the background. */
  async #fork(text: string): Promise<ThreadSession | null> {
    // The thread this question is about. The panel can move on while the
    // fork is awaited; the fork still belongs to this one.
    const parent = this.#parent;
    let session: ThreadSession | null = null;
    try {
      const { id } = await forkThread(parent);
      session = openSession(id);
      // The turns the fork carries at this point are the parent's, and the
      // panel hides exactly that many.
      await session.load();
      if (!session.thread) throw new Error(session.error ?? "Could not read the side question.");
      const inherited = session.thread.turns.length;
      session.inheritedTurns = inherited;
      this.#host.onDataChanged(await commands.addSideQuestion(parent, id, text, inherited));
      const forked = session;
      session = null;
      if (this.#disposed) {
        // File the question under its own thread and let it run, without
        // touching the panel that has moved on. The registry keeps the
        // session while the turn runs.
        void forked.send(textInput(text)).finally(() => releaseSession(forked));
        return null;
      }
      this.#letGo();
      this.openId = id;
      this.#session = forked;
      return forked;
    } catch (cause) {
      if (!this.#disposed) this.#error = errorMessage(cause);
      return null;
    } finally {
      if (session) releaseSession(session);
    }
  }

  async stop(): Promise<void> {
    const session = this.#session;
    if (!session?.activeTurn) return;
    // Waits for the real turn id when the question is still being started.
    await session.interrupt();
    const id = session.id;
    if (this.#session !== session || !session.activeTurn || !id) return;
    // Stop is final: if no `turn/completed` confirms it, end the turn here so
    // the composer comes back.
    clearTimeout(this.#stopTimer);
    this.#stopTimer = setTimeout(() => {
      const thread = session.thread;
      if (this.#session !== session || !thread || !session.activeTurn) return;
      finalizeRunningTurns(thread.turns, "interrupted");
      commands.invalidateThreadCache(id).catch(() => {});
    }, STOP_GRACE_MS);
  }

  async remove(entry: SideQuestion): Promise<void> {
    try {
      this.#host.onDataChanged(await commands.removeSideQuestion(entry.sideThreadId));
      if (this.openId === entry.sideThreadId) this.open(null);
    } catch (cause) {
      if (!this.#disposed) this.#error = errorMessage(cause);
    }
  }

  dispose(): void {
    this.#disposed = true;
    this.#letGo();
  }

  /** Hand the open side question's session back to the registry. */
  #letGo(): void {
    clearTimeout(this.#stopTimer);
    const session = this.#session;
    this.#session = null;
    if (!session) return;
    // The panel has shown its error; left standing it would keep the session
    // retained as "working" with nothing to show it.
    session.streamError = null;
    releaseSession(session);
  }
}
