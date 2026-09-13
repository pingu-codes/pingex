import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SideExchange, SideQuestion } from "$lib/types";

const mocks = vi.hoisted(() => ({
  sideQuestionMode: vi.fn(),
  addNativeSideQuestion: vi.fn(),
  readNativeSideQuestion: vi.fn(),
  prepareNativeSideQuestion: vi.fn(),
  answerNativeSideQuestion: vi.fn(),
  cancelNativeSideQuestion: vi.fn(),
  removeSideQuestion: vi.fn(),
  forkThread: vi.fn(),
  readThread: vi.fn(),
  startTurn: vi.fn(),
  interruptTurn: vi.fn(),
}));
vi.mock("$lib/services/api", async (original) => ({
  ...(await original<typeof import("$lib/services/api")>()),
  ...mocks,
}));

import SideQuestions from "$lib/panels/SideQuestions.svelte";
import { activeTurns } from "$lib/services/codexEvents.svelte";

const side: SideQuestion = {
  kind: "native",
  sideThreadId: "side",
  parentThreadId: "parent",
  title: "Earlier question",
  createdAt: 1,
};
const exchange = (outcome = "completed", response: string | null = "An answer"): SideExchange => ({
  id: "request",
  question: "Why?",
  response,
  outcome,
  synthetic: false,
  refusal: null,
  createdAt: 1,
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
function setup(extra = {}) {
  return render(SideQuestions, {
    parentThreadId: "parent",
    sideQuestions: [side],
    activeSideId: "side",
    onDataChanged: vi.fn(),
    ...extra,
  });
}

describe("native side questions", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mocks.sideQuestionMode.mockResolvedValue("native");
    mocks.readNativeSideQuestion.mockResolvedValue([]);
    mocks.prepareNativeSideQuestion.mockResolvedValue("request");
    mocks.answerNativeSideQuestion.mockResolvedValue([exchange()]);
    mocks.addNativeSideQuestion.mockResolvedValue({ sideQuestions: [side] });
    activeTurns.list = [];
  });

  it("asks while the parent runs, without forking or starting a main turn", async () => {
    activeTurns.list = ["parent"];
    setup();
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");
    expect(await screen.findByText("An answer")).toBeInTheDocument();
    expect(mocks.answerNativeSideQuestion).toHaveBeenCalledWith("parent", "side", "request");
    expect(mocks.forkThread).not.toHaveBeenCalled();
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(mocks.readThread).not.toHaveBeenCalled();
    expect(activeTurns.list).toEqual(["parent"]);
  });

  it("reopens saved history and sends another question on the same conversation", async () => {
    mocks.readNativeSideQuestion.mockResolvedValue([exchange()]);
    setup();
    expect(await screen.findByText("An answer")).toBeInTheDocument();
    const user = userEvent.setup();
    await user.type(screen.getByPlaceholderText("Follow up…"), "Explain further{Enter}");
    await waitFor(() =>
      expect(mocks.prepareNativeSideQuestion).toHaveBeenCalledWith("parent", "side", "Explain further"),
    );
    expect(mocks.addNativeSideQuestion).not.toHaveBeenCalled();
  });

  it("waits for a request identity before cancelling only that side question", async () => {
    const prepare = deferred<string>();
    mocks.prepareNativeSideQuestion.mockReturnValue(prepare.promise);
    mocks.cancelNativeSideQuestion.mockResolvedValue([exchange("interrupted", null)]);
    setup();
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");
    await user.click(await screen.findByRole("button", { name: "Stop side question" }));
    expect(mocks.cancelNativeSideQuestion).not.toHaveBeenCalled();
    prepare.resolve("request");
    expect(await screen.findByText("Side question stopped.")).toBeInTheDocument();
    expect(mocks.cancelNativeSideQuestion).toHaveBeenCalledWith("parent", "side", "request");
    expect(mocks.answerNativeSideQuestion).not.toHaveBeenCalled();
    expect(mocks.interruptTurn).not.toHaveBeenCalled();
  });

  it("restores the draft after an error", async () => {
    mocks.answerNativeSideQuestion.mockResolvedValue([exchange("error", "Update Claude Code")]);
    setup();
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");
    expect(await screen.findByText("Update Claude Code")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("Follow up…")).toHaveValue("Why?");
  });

  it("does not put a late answer into the newly selected parent's panel", async () => {
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

  it("submits a /btw question once through the native flow", async () => {
    setup({ activeSideId: null, submission: { id: 1, text: "Why?" } });
    expect(await screen.findByText("An answer")).toBeInTheDocument();
    expect(mocks.addNativeSideQuestion).toHaveBeenCalledOnce();
    expect(mocks.prepareNativeSideQuestion).toHaveBeenCalledOnce();
    expect(mocks.answerNativeSideQuestion).toHaveBeenCalledOnce();
  });

  it("unlocks the list composer when leaving a conversation whose read is pending", async () => {
    const read = deferred<SideExchange[]>();
    mocks.readNativeSideQuestion.mockReturnValueOnce(read.promise);
    const view = setup();
    await screen.findByLabelText("Loading side question");
    await view.rerender({ activeSideId: null });
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Ask a side question…"), "New question");
    expect(screen.getByRole("button", { name: "Ask side question" })).toBeEnabled();
    read.resolve([exchange()]);
    await waitFor(() => expect(screen.queryByText("An answer")).not.toBeInTheDocument());
  });

  it("loads another saved conversation while the first answer is still running", async () => {
    const answer = deferred<SideExchange[]>();
    mocks.answerNativeSideQuestion.mockReturnValueOnce(answer.promise);
    mocks.readNativeSideQuestion.mockImplementation((_parent, id) =>
      Promise.resolve(id === "other-side" ? [{ ...exchange(), response: "Saved second answer" }] : []),
    );
    const questions = [side, { ...side, sideThreadId: "other-side", title: "Other side conversation" }];
    const view = setup({ sideQuestions: questions });
    const user = userEvent.setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");
    await waitFor(() => expect(mocks.answerNativeSideQuestion).toHaveBeenCalled());
    await view.rerender({ activeSideId: "other-side" });
    expect(await screen.findByText("Saved second answer")).toBeInTheDocument();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    answer.resolve([exchange()]);
    await waitFor(() => expect(screen.queryByText("An answer")).not.toBeInTheDocument());
    expect(screen.getByText("Saved second answer")).toBeInTheDocument();
  });
});
