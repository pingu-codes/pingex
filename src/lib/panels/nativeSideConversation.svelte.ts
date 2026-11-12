/**
 * The native adapter of a side question's conversation: the harness answers
 * in place, from what the parent thread already knows and without tools
 * (Claude Code's side questions today). No thread is forked and no turn runs,
 * so the parent's own turn keeps going untouched.
 *
 * A question is two steps on the backend: `prepare` gives it a request id,
 * then `answer` waits for the reply. Stop cancels by that id, so a Stop
 * pressed before it exists waits for it. The history is the harness's and is
 * read back on open; an answer still pending from earlier is polled for.
 */

import type { SideConversation, SideConversationHost, SideTranscript } from "$lib/panels/sideConversation.svelte";
import { commands, identity, scopedId } from "$lib/services/homeRouting";
import type { SideExchange, SideQuestion } from "$lib/types";

const POLL_MS = 1000;

type Flight = { cancelled: boolean; prepared: Promise<{ id: string; request: string }> };

export class NativeSideConversation implements SideConversation {
  readonly about = {
    text: "Ask about what the agent already knows from this conversation. Side questions cannot read files or use tools, and stay out of the main conversation.",
    pinned: true,
  };
  openId = $state<string | null>(null);
  #exchanges = $state<SideExchange[]>([]);
  #error = $state<string | null>(null);
  #loading = $state(false);
  #starting = $state(false);
  #startingSideId = $state<string | null>(null);
  /** Bumped by every write to `#exchanges`, so a read that was overtaken
   *  drops its result. */
  #readRevision = 0;
  #flight: Flight | null = null;
  #disposed = false;
  readonly #parent: string;
  readonly #host: SideConversationHost;
  readonly #stopPolling: () => void;

  constructor(parentThreadId: string, host: SideConversationHost) {
    this.#parent = parentThreadId;
    this.#host = host;
    this.#stopPolling = $effect.root(() => {
      $effect(() => this.#pollWhilePending());
    });
  }

  get entries(): SideQuestion[] {
    return this.#host
      .sideQuestions()
      .filter((entry) => entry.parentThreadId === this.#parent && entry.kind === "native");
  }

  get transcript(): SideTranscript | null {
    return this.openId ? { shape: "exchanges", exchanges: this.#exchanges } : null;
  }

  get loading(): boolean {
    return this.#loading;
  }

  get busy(): boolean {
    return this.#startingHere || !!this.#pending;
  }

  get error(): string | null {
    return this.#error;
  }

  get #pending(): SideExchange | undefined {
    return this.#exchanges.find((entry) => entry.outcome === "pending");
  }

  /** A question is being asked on the side question on show (or on a new
   *  one, from the list). One asked elsewhere does not lock this one. */
  get #startingHere(): boolean {
    return this.#starting && (!this.openId || this.openId === this.#startingSideId);
  }

  #live(id: string | null): boolean {
    return !this.#disposed && this.openId === id;
  }

  open(id: string | null): void {
    if (this.#disposed || id === this.openId) return;
    this.#select(id);
  }

  #select(id: string | null): void {
    this.openId = id;
    this.#readRevision++;
    this.#loading = false;
    this.#exchanges = [];
    this.#error = null;
    if (id) void this.#load(id);
  }

  async #load(id: string, showLoading = true): Promise<void> {
    const revision = ++this.#readRevision;
    const current = () => this.#live(id) && revision === this.#readRevision;
    if (showLoading) this.#loading = true;
    try {
      const result = await commands.readNativeSideQuestion(this.#parent, id);
      if (current()) this.#exchanges = result;
    } catch (cause) {
      if (current()) this.#error = String(cause);
    } finally {
      if (current()) this.#loading = false;
    }
  }

  // Poll one read at a time. Each `load` supersedes the one before it, so on a
  // fixed interval a read slower than the interval was always stale on arrival
  // and the answer never showed.
  #pollWhilePending(): (() => void) | undefined {
    if (!this.#pending || this.#startingHere || !this.openId) return;
    const id = this.openId;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      await this.#load(id, false);
      if (!stopped) timer = setTimeout(poll, POLL_MS);
    };
    timer = setTimeout(poll, POLL_MS);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }

  async ask(text: string): Promise<boolean> {
    if (this.#disposed || this.busy || this.#loading) return false;
    const parent = this.#parent;
    const selectionAtStart = this.openId;
    const selected =
      this.openId && this.entries.some((entry) => entry.sideThreadId === this.openId) ? this.openId : null;
    let originId = selected;
    let restore = false;
    this.#error = null;
    this.#starting = true;
    this.#startingSideId = selected;
    this.#readRevision++;
    const operation: Flight = {
      cancelled: false,
      prepared: (async () => {
        let id = selected;
        if (!id) {
          const nativeId = crypto.randomUUID();
          const home = identity(parent)?.homeKey;
          id = home ? scopedId(home, nativeId) : nativeId;
          originId = id;
          this.#startingSideId = id;
          this.#host.onDataChanged(await commands.addNativeSideQuestion(parent, id, text));
          if (this.#live(selectionAtStart)) this.#select(id);
        }
        const request = await commands.prepareNativeSideQuestion(parent, id, text);
        return { id, request };
      })(),
    };
    this.#flight = operation;
    let prepared: { id: string; request: string } | null = null;
    try {
      const { id, request } = await operation.prepared;
      prepared = { id, request };
      if (this.#live(id)) {
        this.#readRevision++;
        this.#loading = false;
        this.#exchanges = [
          ...this.#exchanges,
          {
            id: request,
            question: text,
            response: null,
            outcome: "pending",
            synthetic: false,
            refusal: null,
            createdAt: Date.now() / 1000,
          },
        ];
      }
      const result = operation.cancelled
        ? await commands.cancelNativeSideQuestion(parent, id, request)
        : await commands.answerNativeSideQuestion(parent, id, request);
      if (this.#live(id)) {
        this.#readRevision++;
        this.#exchanges = result;
        restore = result.find((entry) => entry.id === request)?.outcome === "error";
      }
    } catch (cause) {
      if (prepared) {
        await commands.cancelNativeSideQuestion(parent, prepared.id, prepared.request).catch(() => {});
      }
      if (!this.#disposed && (!this.openId || this.openId === (prepared?.id ?? originId))) {
        this.#readRevision++;
        this.#exchanges = this.#exchanges.map((entry) =>
          entry.id === prepared?.request ? { ...entry, outcome: "error", response: String(cause) } : entry,
        );
        this.#error = String(cause);
        restore = true;
      }
    } finally {
      if (this.#flight === operation) {
        this.#flight = null;
        if (!this.#disposed) this.#starting = false;
      }
    }
    return !restore;
  }

  async stop(): Promise<void> {
    const parent = this.#parent;
    const operation = this.#startingHere ? this.#flight : null;
    if (operation) operation.cancelled = true;
    const pending = this.#pending;
    try {
      const target = operation
        ? await operation.prepared
        : pending && this.openId
          ? { id: this.openId, request: pending.id }
          : null;
      if (!target) return;
      const result = await commands.cancelNativeSideQuestion(parent, target.id, target.request);
      if (this.#live(target.id)) {
        this.#readRevision++;
        this.#exchanges = result;
      }
    } catch (cause) {
      if (!this.#disposed) this.#error = String(cause);
    }
  }

  async remove(entry: SideQuestion): Promise<void> {
    try {
      this.#host.onDataChanged(await commands.removeSideQuestion(entry.sideThreadId));
      if (this.#live(entry.sideThreadId)) this.#select(null);
    } catch (cause) {
      if (!this.#disposed) this.#error = String(cause);
    }
  }

  dispose(): void {
    this.#disposed = true;
    this.#stopPolling();
  }
}
