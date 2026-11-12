// The panel's wiring: how it lays out each transcript shape and moves text
// and selection between the composer and the conversation. What a side
// question does is tested once per adapter in `sideConversation.test.ts`.
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SideExchange, SideQuestion } from "$lib/types";

const mocks = vi.hoisted(() => ({
  readThread: vi.fn(),
  startTurn: vi.fn(),
  forkThread: vi.fn(),
  sideQuestionMode: vi.fn(),
  interruptTurn: vi.fn(),
  addSideQuestion: vi.fn(),
  removeSideQuestion: vi.fn(),
  invalidateThreadCache: vi.fn(),
  addNativeSideQuestion: vi.fn(),
  readNativeSideQuestion: vi.fn(),
  prepareNativeSideQuestion: vi.fn(),
  answerNativeSideQuestion: vi.fn(),
  cancelNativeSideQuestion: vi.fn(),
}));

vi.mock("$lib/services/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/services/api")>()),
  readThread: mocks.readThread,
  startTurn: mocks.startTurn,
  forkThread: mocks.forkThread,
}));
vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  const { readThread: _read, startTurn: _start, forkThread: _fork, ...commands } = mocks;
  return { ...actual, commands: { ...actual.commands, ...commands } };
});

import SideQuestions from "$lib/panels/SideQuestions.svelte";
import { activeTurns, approvals, previewEmit } from "$lib/services/codexEvents.svelte";
import { resetSessions } from "$lib/thread/sessions.svelte";

const askButton = () => screen.getByRole("button", { name: "Ask side question" });
const stopButton = () => screen.queryByRole("button", { name: "Stop side question" });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

beforeEach(() => {
  vi.resetAllMocks();
  mocks.invalidateThreadCache.mockResolvedValue(undefined);
  activeTurns.list = [];
  approvals.list = [];
  resetSessions();
});

describe("SideQuestions on a fork", () => {
  const SIDE = "side-1";
  const sideQuestions: SideQuestion[] = [
    { sideThreadId: SIDE, parentThreadId: "parent-1", title: "Why trailing edge?", createdAt: 10, inheritedTurns: 0 },
  ];

  beforeEach(() => {
    mocks.sideQuestionMode.mockResolvedValue("fork");
    mocks.readThread.mockResolvedValue({ id: SIDE, preview: "", cwd: "/project", turns: [] });
    mocks.startTurn.mockResolvedValue({ id: "turn-1", status: "inProgress", items: [] });
    mocks.interruptTurn.mockResolvedValue(undefined);
  });

  it("lays out the running turn with its work, approvals and working indicator", async () => {
    const user = userEvent.setup();
    render(SideQuestions, { parentThreadId: "parent-1", sideQuestions, activeSideId: SIDE, onDataChanged: vi.fn() });
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Run it{Enter}");

    expect(await screen.findByLabelText("Codex is working")).toBeInTheDocument();
    expect(stopButton()).toBeInTheDocument();
    expect(askButton()).toBeDisabled();

    previewEmit({
      method: "item/started",
      params: {
        threadId: SIDE,
        turnId: "turn-1",
        item: { type: "commandExecution", id: "cmd-1", command: "deno task check", cwd: "/project" },
      },
    });
    expect(await screen.findByText("deno task check", { selector: "code" })).toBeInTheDocument();

    approvals.list = [
      {
        requestId: 7,
        kind: "command",
        threadId: SIDE,
        turnId: "turn-1",
        itemId: "cmd-1",
        command: "rm -rf build",
        cwd: "/project",
      },
    ];
    expect(await screen.findByText("Codex wants to run a command")).toBeInTheDocument();
    expect(screen.queryByLabelText("Codex is working")).not.toBeInTheDocument();

    approvals.list = [];
    previewEmit({
      method: "turn/completed",
      params: { threadId: SIDE, turn: { id: "turn-1", status: "completed", items: [] } },
    });
    await waitFor(() => expect(stopButton()).not.toBeInTheDocument());
    expect(screen.queryByLabelText("Codex is working")).not.toBeInTheDocument();
  });
});

describe("SideQuestions answered natively", () => {
  const side: SideQuestion = {
    kind: "native",
    sideThreadId: "side",
    parentThreadId: "parent",
    title: "Earlier question",
    createdAt: 1,
  };
  const exchange = (outcome = "completed", response: string | null = "An answer", id = "request"): SideExchange => ({
    id,
    question: "Why?",
    response,
    outcome,
    synthetic: false,
    refusal: null,
    createdAt: 1,
  });
  const setup = (extra = {}) =>
    render(SideQuestions, {
      parentThreadId: "parent",
      sideQuestions: [side],
      activeSideId: "side",
      onDataChanged: vi.fn(),
      ...extra,
    });

  beforeEach(() => {
    mocks.sideQuestionMode.mockResolvedValue("native");
    mocks.readNativeSideQuestion.mockResolvedValue([]);
    mocks.prepareNativeSideQuestion.mockResolvedValue("request");
    mocks.answerNativeSideQuestion.mockResolvedValue([exchange()]);
    mocks.addNativeSideQuestion.mockResolvedValue({ sideQuestions: [side] });
  });

  it("keeps its note on show and lays out answers, refusals and stops", async () => {
    mocks.readNativeSideQuestion.mockResolvedValue([
      {
        ...exchange(),
        refusal: { originalModel: "opus", fallbackModel: "sonnet", content: "Answered from memory only" },
      },
      exchange("interrupted", null, "stopped"),
    ]);
    setup();

    expect(await screen.findByText("An answer")).toBeInTheDocument();
    expect(screen.getByText("Answered from memory only")).toBeInTheDocument();
    expect(screen.getByText("Side question stopped.")).toBeInTheDocument();
    expect(screen.getByText(/Side questions cannot read files or use tools/)).toBeInTheDocument();
  });

  it("gives a failed question back to the composer", async () => {
    mocks.answerNativeSideQuestion.mockResolvedValue([exchange("error", "Update Claude Code")]);
    setup();
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");

    expect(await screen.findByText("Update Claude Code")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Follow up…")).toHaveValue("Why?");
  });

  it("asks a /btw question once", async () => {
    setup({ activeSideId: null, submission: { id: 1, text: "Why?" } });

    expect(await screen.findByText("An answer")).toBeInTheDocument();
    expect(mocks.addNativeSideQuestion).toHaveBeenCalledOnce();
    expect(mocks.prepareNativeSideQuestion).toHaveBeenCalledOnce();
    expect(mocks.answerNativeSideQuestion).toHaveBeenCalledOnce();
  });

  it("does not put a late answer into the next parent's panel", async () => {
    const answer = deferred<SideExchange[]>();
    mocks.answerNativeSideQuestion.mockReturnValue(answer.promise);
    const view = setup();
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");
    await waitFor(() => expect(mocks.answerNativeSideQuestion).toHaveBeenCalled());

    await view.rerender({ parentThreadId: "other", sideQuestions: [], activeSideId: null, onDataChanged: vi.fn() });
    await screen.findByPlaceholderText("Ask a side question…");
    answer.resolve([exchange()]);

    await waitFor(() => expect(screen.queryByText("An answer")).not.toBeInTheDocument());
    expect(mocks.prepareNativeSideQuestion).toHaveBeenCalledOnce();
  });

  it("unlocks the list composer when Back leaves a conversation whose read is pending", async () => {
    const read = deferred<SideExchange[]>();
    mocks.readNativeSideQuestion.mockReturnValueOnce(read.promise);
    const view = setup();
    await screen.findByLabelText("Loading side question");

    await view.rerender({ activeSideId: null });
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Ask a side question…"), "New question");
    expect(askButton()).toBeEnabled();
    read.resolve([exchange()]);
    await waitFor(() => expect(screen.queryByText("An answer")).not.toBeInTheDocument());
  });
});
