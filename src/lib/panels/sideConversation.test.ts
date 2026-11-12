import { waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SideExchange, SideQuestion, ThreadDetail, Turn } from "$lib/types";

const mocks = vi.hoisted(() => ({
  // Both adapters
  sideQuestionMode: vi.fn(),
  removeSideQuestion: vi.fn(),
  // Fork
  forkThread: vi.fn(),
  readThread: vi.fn(),
  startTurn: vi.fn(),
  interruptTurn: vi.fn(),
  addSideQuestion: vi.fn(),
  invalidateThreadCache: vi.fn(),
  // Native
  addNativeSideQuestion: vi.fn(),
  readNativeSideQuestion: vi.fn(),
  prepareNativeSideQuestion: vi.fn(),
  answerNativeSideQuestion: vi.fn(),
  cancelNativeSideQuestion: vi.fn(),
}));

vi.mock("$lib/services/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/services/api")>()),
  forkThread: mocks.forkThread,
  readThread: mocks.readThread,
  startTurn: mocks.startTurn,
}));
vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  const { forkThread: _fork, readThread: _read, startTurn: _start, ...commands } = mocks;
  return { ...actual, commands: { ...actual.commands, ...commands } };
});

import { openSideConversation, type SideConversation, type SideTranscript } from "$lib/panels/sideConversation.svelte";
import { activeTurns, previewEmit } from "$lib/services/codexEvents.svelte";
import { openSession, peekSession, releaseSession, resetSessions } from "$lib/thread/sessions.svelte";
import { messageParts } from "$lib/thread/turnSegments";

const PARENT = "parent";
const SIDE = "side";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (cause: unknown) => void;
  const promise = new Promise<T>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

/** How one adapter's backend is driven from a test. */
interface Rig {
  /** A side question of this kind, saved under `PARENT`. */
  saved(id: string): SideQuestion;
  /** What reading a saved side question returns: these question/answer pairs. */
  history(id: string, pairs: [string, string][]): void;
  /** Hold the next question before the harness has an id for it. */
  holdStart(): () => void;
  /** Resolves once the harness is answering the question asked last. */
  asking(): Promise<void>;
  /** The answer to the question asked last arrives. */
  answer(text: string): void;
  /** The next question fails on its way to the harness. */
  failNext(message: string): void;
  /** How many side questions were filed. */
  filed(): number;
  /** What each Stop reached the harness with, and what it should have been. */
  stops(): { sent: unknown[][]; expected: unknown[] };
  /** The transcript's questions and answers, in order. */
  texts(transcript: SideTranscript | null): string[];
}

function userTurn(id: string, question: string, answer: string | null, status = "completed"): Turn {
  return {
    id,
    status,
    items: [
      { type: "userMessage", id: `${id}-user`, content: [{ type: "text", text: question }] },
      ...(answer === null ? [] : [{ type: "agentMessage" as const, id: `${id}-agent`, text: answer }]),
    ],
  };
}

/** A fork that inherited one turn from its parent, which the panel hides. */
function forkRig(): Rig {
  const threads = new Map<string, Turn[]>();
  let started = 0;
  let current: { threadId: string; turnId: string } | null = null;
  let hold: Promise<void> | null = null;
  let failure: string | null = null;
  const asked = () => deferred<void>();
  let askedSignal = asked();
  const inherited = () => userTurn("parent-turn", "Parent question", "Parent answer");

  mocks.sideQuestionMode.mockResolvedValue("fork");
  mocks.forkThread.mockResolvedValue({ id: "fork-new" });
  mocks.readThread.mockImplementation(
    async (id: string): Promise<ThreadDetail> => ({
      id,
      preview: "",
      cwd: "/project",
      turns: structuredClone(threads.get(id) ?? [inherited()]),
    }),
  );
  mocks.addSideQuestion.mockResolvedValue({ sideQuestions: [] });
  mocks.startTurn.mockImplementation(async (threadId: string) => {
    const turnId = `turn-${++started}`;
    if (failure) {
      const message = failure;
      failure = null;
      throw new Error(message);
    }
    if (hold) await hold;
    current = { threadId, turnId };
    askedSignal.resolve();
    return { id: turnId, status: "inProgress", items: [] };
  });
  // As Codex does: an interrupt ends the turn with a completion.
  mocks.interruptTurn.mockImplementation(async (threadId: string, turnId: string) => {
    previewEmit({
      method: "turn/completed",
      params: { threadId, turn: { id: turnId, status: "interrupted", items: [] } },
    });
  });

  return {
    saved: (id) => ({ sideThreadId: id, parentThreadId: PARENT, title: "Saved", createdAt: 1, inheritedTurns: 1 }),
    history(id, pairs) {
      threads.set(id, [inherited(), ...pairs.map(([q, a], index) => userTurn(`saved-${index}`, q, a))]);
    },
    holdStart() {
      const gate = deferred<void>();
      hold = gate.promise;
      return () => {
        hold = null;
        gate.resolve();
      };
    },
    async asking() {
      await askedSignal.promise;
      askedSignal = asked();
    },
    answer(text) {
      if (!current) throw new Error("nothing asked");
      const { threadId, turnId } = current;
      previewEmit({
        method: "item/completed",
        params: { threadId, turnId, item: { type: "agentMessage", id: `${turnId}-agent`, text } },
      });
      previewEmit({
        method: "turn/completed",
        params: { threadId, turn: { id: turnId, status: "completed", items: [] } },
      });
    },
    failNext(message) {
      failure = message;
    },
    filed: () => mocks.addSideQuestion.mock.calls.length,
    stops: () => ({ sent: mocks.interruptTurn.mock.calls, expected: [current?.threadId, current?.turnId] }),
    texts(transcript) {
      if (transcript?.shape !== "turns") return [];
      return transcript.turns.flatMap((turn) =>
        turn.items.flatMap((item) => {
          if (item.type === "userMessage")
            return [
              messageParts(item)
                .map((part) => part.text ?? "")
                .join(""),
            ];
          if (item.type === "agentMessage") return [item.text ?? ""];
          return [];
        }),
      );
    },
  };
}

/** The harness answering in place, keeping each side question's exchanges. */
function nativeRig(): Rig {
  const logs = new Map<string, SideExchange[]>();
  const log = (id: string) => logs.get(id) ?? logs.set(id, []).get(id)!;
  let requests = 0;
  let current: { id: string; request: string; reply: ReturnType<typeof deferred<SideExchange[]>> } | null = null;
  let hold: Promise<void> | null = null;
  let failure: string | null = null;
  let askedSignal = deferred<void>();
  const exchange = (id: string, question: string, response: string | null, outcome: string): SideExchange => ({
    id,
    question,
    response,
    outcome,
    synthetic: false,
    refusal: null,
    createdAt: 1,
  });

  mocks.sideQuestionMode.mockResolvedValue("native");
  mocks.addNativeSideQuestion.mockResolvedValue({ sideQuestions: [] });
  mocks.readNativeSideQuestion.mockImplementation(async (_parent: string, id: string) => structuredClone(log(id)));
  mocks.prepareNativeSideQuestion.mockImplementation(async (_parent: string, id: string, question: string) => {
    if (hold) await hold;
    const request = `request-${++requests}`;
    log(id).push(exchange(request, question, null, "pending"));
    return request;
  });
  mocks.answerNativeSideQuestion.mockImplementation((_parent: string, id: string, request: string) => {
    const reply = deferred<SideExchange[]>();
    current = { id, request, reply };
    if (failure) {
      reply.reject(new Error(failure));
      failure = null;
    }
    askedSignal.resolve();
    return reply.promise;
  });
  mocks.cancelNativeSideQuestion.mockImplementation(async (_parent: string, id: string, request: string) => {
    const entry = log(id).find((candidate) => candidate.id === request);
    if (entry) entry.outcome = "interrupted";
    return structuredClone(log(id));
  });

  return {
    saved: (id) => ({ kind: "native", sideThreadId: id, parentThreadId: PARENT, title: "Saved", createdAt: 1 }),
    history(id, pairs) {
      logs.set(
        id,
        pairs.map(([q, a], index) => exchange(`saved-${index}`, q, a, "completed")),
      );
    },
    holdStart() {
      const gate = deferred<void>();
      hold = gate.promise;
      return () => {
        hold = null;
        gate.resolve();
      };
    },
    async asking() {
      await askedSignal.promise;
      askedSignal = deferred<void>();
    },
    answer(text) {
      if (!current) throw new Error("nothing asked");
      const entry = log(current.id).find((candidate) => candidate.id === current?.request);
      if (entry) Object.assign(entry, { response: text, outcome: "completed" });
      current.reply.resolve(structuredClone(log(current.id)));
    },
    failNext(message) {
      failure = message;
    },
    filed: () => mocks.addNativeSideQuestion.mock.calls.length,
    stops: () => ({
      sent: mocks.cancelNativeSideQuestion.mock.calls,
      expected: [PARENT, SIDE, `request-${requests}`],
    }),
    texts(transcript) {
      if (transcript?.shape !== "exchanges") return [];
      return transcript.exchanges.flatMap((entry) => [entry.question, ...(entry.response ? [entry.response] : [])]);
    },
  };
}

let conversation: SideConversation | null = null;
let onDataChanged = vi.fn();

async function open(rig: Rig, sideQuestions?: SideQuestion[]): Promise<SideConversation> {
  const entries = sideQuestions ?? [rig.saved(SIDE)];
  conversation = await openSideConversation(PARENT, { sideQuestions: () => entries, onDataChanged });
  return conversation;
}

async function openSaved(rig: Rig): Promise<SideConversation> {
  const side = await open(rig);
  side.open(SIDE);
  await waitFor(() => expect(side.loading).toBe(false));
  return side;
}

beforeEach(() => {
  vi.resetAllMocks();
  mocks.invalidateThreadCache.mockResolvedValue(undefined);
  mocks.removeSideQuestion.mockResolvedValue({ sideQuestions: [] });
  activeTurns.list = [];
  onDataChanged = vi.fn();
  resetSessions();
});

afterEach(() => {
  conversation?.dispose();
  conversation = null;
});

describe.each([
  ["fork", forkRig],
  ["native", nativeRig],
])("a %s side question", (_name, makeRig) => {
  it("starts from the list, files itself under the parent and shows the answer", async () => {
    const rig = makeRig();
    const side = await open(rig, []);
    expect(side.openId).toBeNull();

    const asked = side.ask("Why?");
    expect(side.busy).toBe(true);
    await rig.asking();
    rig.answer("Because.");

    expect(await asked).toBe(true);
    await waitFor(() => expect(side.busy).toBe(false));
    expect(side.openId).not.toBeNull();
    expect(rig.texts(side.transcript)).toEqual(["Why?", "Because."]);
    expect(rig.filed()).toBe(1);
    expect(onDataChanged).toHaveBeenCalled();
  });

  it("follows up on a saved side question without filing a new one", async () => {
    const rig = makeRig();
    rig.history(SIDE, [["Earlier?", "Earlier answer."]]);
    const side = await openSaved(rig);
    expect(rig.texts(side.transcript)).toEqual(["Earlier?", "Earlier answer."]);

    const asked = side.ask("More?");
    await rig.asking();
    rig.answer("More answer.");

    expect(await asked).toBe(true);
    await waitFor(() =>
      expect(rig.texts(side.transcript)).toEqual(["Earlier?", "Earlier answer.", "More?", "More answer."]),
    );
    expect(side.busy).toBe(false);
    expect(rig.filed()).toBe(0);
  });

  it("holds a Stop pressed before the harness has the question, then stops that question", async () => {
    const rig = makeRig();
    const side = await openSaved(rig);
    const release = rig.holdStart();

    void side.ask("Why?");
    await waitFor(() => expect(side.busy).toBe(true));
    const stopping = side.stop();
    await Promise.resolve();
    expect(rig.stops().sent).toHaveLength(0);

    release();
    await stopping;
    await waitFor(() => expect(side.busy).toBe(false));
    const { sent, expected } = rig.stops();
    expect(sent.length).toBeGreaterThan(0);
    for (const call of sent) expect(call).toEqual(expected);
  });

  it("gives a failed question back", async () => {
    const rig = makeRig();
    const side = await openSaved(rig);
    rig.failNext("The harness is gone");

    expect(await side.ask("Why?")).toBe(false);
    expect(side.error).toContain("The harness is gone");
    expect(side.busy).toBe(false);
  });

  it("goes back to the list when the open side question is deleted", async () => {
    const rig = makeRig();
    const side = await openSaved(rig);

    await side.remove(rig.saved(SIDE));

    expect(mocks.removeSideQuestion).toHaveBeenCalledWith(SIDE);
    expect(onDataChanged).toHaveBeenCalled();
    expect(side.openId).toBeNull();
    expect(side.transcript).toBeNull();
  });

  it("lists only its parent's side questions", async () => {
    const rig = makeRig();
    const side = await open(rig, [rig.saved(SIDE), { ...rig.saved("elsewhere"), parentThreadId: "other-parent" }]);

    expect(side.entries.map((entry) => entry.sideThreadId)).toEqual([SIDE]);
  });

  it("shows nothing that lands after the panel let it go", async () => {
    const rig = makeRig();
    const side = await openSaved(rig);
    const asked = side.ask("Why?");
    await rig.asking();

    side.dispose();
    rig.answer("Too late.");
    await asked;

    expect(rig.texts(side.transcript)).not.toContain("Too late.");
  });
});

describe("a fork side question", () => {
  it("does not hold the composer on a parent turn it inherited mid-turn", async () => {
    const rig = forkRig();
    // The fork is working, so a dead session cannot explain the running
    // inherited turn away: it is the parent's, recorded mid-turn.
    activeTurns.list = [SIDE];
    mocks.readThread.mockResolvedValue({
      id: SIDE,
      preview: "",
      cwd: "/project",
      turns: [userTurn("parent-turn", "Parent work", null, "inProgress")],
    });
    const side = await openSaved(rig);

    expect(side.busy).toBe(false);
    expect(rig.texts(side.transcript)).toEqual([]);
    void side.ask("Why?");
    await rig.asking();
    expect(mocks.startTurn.mock.calls[0]?.slice(0, 2)).toEqual([SIDE, [{ type: "text", text: "Why?" }]]);
  });

  it("ends the turn itself when Stop gets no completion back", async () => {
    const rig = forkRig();
    mocks.interruptTurn.mockResolvedValue(undefined);
    const side = await openSaved(rig);
    void side.ask("Why?");
    await rig.asking();

    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      await side.stop();
      expect(mocks.interruptTurn).toHaveBeenCalledWith(SIDE, "turn-1");
      expect(side.busy).toBe(true);
      await vi.advanceTimersByTimeAsync(1500);
      expect(side.busy).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps its session apart from the parent's and lets it go once idle", async () => {
    const rig = forkRig();
    const parent = openSession(PARENT);
    await waitFor(() => expect(parent.loading).toBe(false));
    const parentTurns = parent.thread?.turns.length;

    const side = await openSaved(rig);
    void side.ask("Why?");
    await rig.asking();
    side.dispose();
    // The answer is still coming: the registry keeps the fork's session.
    expect(peekSession(SIDE)).not.toBeNull();

    rig.answer("Because.");
    await waitFor(() => expect(peekSession(SIDE)).toBeNull());
    expect(peekSession(PARENT)).toBe(parent);
    expect(parent.thread?.turns.length).toBe(parentTurns);
    releaseSession(parent);
  });

  it("still asks a question the panel moved on from while forking", async () => {
    const rig = forkRig();
    const fork = deferred<{ id: string }>();
    mocks.forkThread.mockReturnValue(fork.promise);
    const side = await open(rig, []);

    const asked = side.ask("Why?");
    side.dispose();
    fork.resolve({ id: "fork-late" });

    expect(await asked).toBe(true);
    await waitFor(() =>
      expect(mocks.startTurn.mock.calls[0]?.slice(0, 2)).toEqual(["fork-late", [{ type: "text", text: "Why?" }]]),
    );
    expect(mocks.addSideQuestion).toHaveBeenCalledWith(PARENT, "fork-late", "Why?", 1);
    expect(side.openId).toBeNull();
  });
});

describe("a native side question", () => {
  it("asks while the parent runs, without forking or starting a turn", async () => {
    const rig = nativeRig();
    activeTurns.list = [PARENT];
    const side = await openSaved(rig);

    const asked = side.ask("Why?");
    await rig.asking();
    rig.answer("An answer");

    expect(await asked).toBe(true);
    expect(mocks.answerNativeSideQuestion).toHaveBeenCalledWith(PARENT, SIDE, "request-1");
    expect(mocks.forkThread).not.toHaveBeenCalled();
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(mocks.readThread).not.toHaveBeenCalled();
    expect(activeTurns.list).toEqual([PARENT]);
  });

  it("gives back a question answered with an error", async () => {
    const rig = nativeRig();
    const side = await openSaved(rig);
    mocks.answerNativeSideQuestion.mockResolvedValueOnce([
      {
        id: "request-1",
        question: "Why?",
        response: "Update Claude Code",
        outcome: "error",
        synthetic: false,
        refusal: null,
        createdAt: 1,
      },
    ]);

    expect(await side.ask("Why?")).toBe(false);
    expect(rig.texts(side.transcript)).toEqual(["Why?", "Update Claude Code"]);
  });

  it("polls for an answer still pending from an earlier session", async () => {
    const rig = nativeRig();
    mocks.readNativeSideQuestion.mockResolvedValueOnce([
      {
        id: "old",
        question: "Why?",
        response: null,
        outcome: "pending",
        synthetic: false,
        refusal: null,
        createdAt: 1,
      },
    ]);
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      const side = await open(rig);
      side.open(SIDE);
      await vi.advanceTimersByTimeAsync(0);
      expect(side.busy).toBe(true);

      mocks.readNativeSideQuestion.mockResolvedValueOnce([
        {
          id: "old",
          question: "Why?",
          response: "Late answer",
          outcome: "completed",
          synthetic: false,
          refusal: null,
          createdAt: 1,
        },
      ]);
      await vi.advanceTimersByTimeAsync(1000);

      expect(rig.texts(side.transcript)).toEqual(["Why?", "Late answer"]);
      expect(side.busy).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens another saved side question while the first is still answering", async () => {
    const rig = nativeRig();
    rig.history("other", [["Second?", "Saved second answer"]]);
    const side = await open(rig, [rig.saved(SIDE), rig.saved("other")]);
    side.open(SIDE);
    await waitFor(() => expect(side.loading).toBe(false));
    void side.ask("Why?");
    await rig.asking();

    side.open("other");
    await waitFor(() => expect(rig.texts(side.transcript)).toEqual(["Second?", "Saved second answer"]));
    expect(side.busy).toBe(false);

    rig.answer("An answer");
    await Promise.resolve();
    expect(rig.texts(side.transcript)).toEqual(["Second?", "Saved second answer"]);
  });
});
