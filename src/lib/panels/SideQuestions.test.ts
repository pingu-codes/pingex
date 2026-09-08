import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SideQuestion, ThreadDetail } from "$lib/types";

const mocks = vi.hoisted(() => ({
  readThread: vi.fn(),
  startTurn: vi.fn(),
  interruptTurn: vi.fn(),
  forkThread: vi.fn(),
  addSideQuestion: vi.fn(),
  removeSideQuestion: vi.fn(),
  invalidateThreadCache: vi.fn(),
}));

vi.mock("$lib/services/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/services/api")>()),
  readThread: mocks.readThread,
  startTurn: mocks.startTurn,
  interruptTurn: mocks.interruptTurn,
  forkThread: mocks.forkThread,
  addSideQuestion: mocks.addSideQuestion,
  removeSideQuestion: mocks.removeSideQuestion,
  invalidateThreadCache: mocks.invalidateThreadCache,
}));

import SideQuestions from "$lib/panels/SideQuestions.svelte";
import { activeTurns, approvals, previewEmit } from "$lib/services/codexEvents.svelte";

const SIDE = "side-1";
const sideQuestions: SideQuestion[] = [
  { sideThreadId: SIDE, parentThreadId: "parent-1", title: "Why trailing edge?", createdAt: 10, inheritedTurns: 0 },
];

function detail(turns: ThreadDetail["turns"] = []): ThreadDetail {
  return { id: SIDE, preview: "", cwd: "/project", turns };
}

function setup(activeSideId: string | null = SIDE) {
  render(SideQuestions, { parentThreadId: "parent-1", sideQuestions, activeSideId, onDataChanged: vi.fn() });
}

const askButton = () => screen.getByRole("button", { name: "Ask side question" });
const stopButton = () => screen.queryByRole("button", { name: "Stop side question" });

describe("SideQuestions", () => {
  beforeEach(() => {
    mocks.readThread.mockReset().mockResolvedValue(detail());
    mocks.startTurn.mockReset().mockResolvedValue({ id: "turn-1", status: "inProgress", items: [] });
    mocks.interruptTurn.mockReset().mockResolvedValue(undefined);
    mocks.invalidateThreadCache.mockReset().mockResolvedValue(undefined);
    activeTurns.list = [];
    approvals.list = [];
  });

  it("shows the working indicator while Codex thinks and clears it on completion", async () => {
    const user = userEvent.setup();
    setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");

    expect(await screen.findByLabelText("Codex is working")).toBeInTheDocument();
    expect(stopButton()).toBeInTheDocument();
    expect(askButton()).toBeDisabled();

    previewEmit({
      method: "turn/completed",
      params: { threadId: SIDE, turn: { id: "turn-1", status: "completed", items: [] } },
    });
    await waitFor(() => expect(screen.queryByLabelText("Codex is working")).not.toBeInTheDocument());
    expect(stopButton()).not.toBeInTheDocument();
  });

  it("renders work items streamed into the side thread", async () => {
    const user = userEvent.setup();
    setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Run it{Enter}");
    await screen.findByLabelText("Codex is working");

    previewEmit({
      method: "item/started",
      params: {
        threadId: SIDE,
        turnId: "turn-1",
        item: { type: "commandExecution", id: "cmd-1", command: "deno task check", cwd: "/project" },
      },
    });
    expect(await screen.findByText("deno task check")).toBeInTheDocument();
  });

  it("shows approvals raised by the side thread", async () => {
    const user = userEvent.setup();
    setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Run it{Enter}");
    await screen.findByLabelText("Codex is working");

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
  });

  it("ends the turn locally when Stop gets no completion back", async () => {
    const user = userEvent.setup();
    setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");
    await screen.findByLabelText("Codex is working");

    await user.click(screen.getByRole("button", { name: "Stop side question" }));
    expect(mocks.interruptTurn).toHaveBeenCalledWith(SIDE, "turn-1");

    await waitFor(() => expect(stopButton()).not.toBeInTheDocument(), { timeout: 3000 });
    expect(screen.queryByLabelText("Codex is working")).not.toBeInTheDocument();
  });

  it("waits for the real turn id before interrupting an optimistic turn", async () => {
    const user = userEvent.setup();
    let resolveStart!: (turn: unknown) => void;
    mocks.startTurn.mockReturnValue(new Promise((resolve) => (resolveStart = resolve)));
    setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");

    await user.click(await screen.findByRole("button", { name: "Stop side question" }));
    expect(mocks.interruptTurn).not.toHaveBeenCalled();
    resolveStart({ id: "turn-9", status: "inProgress", items: [] });
    await waitFor(() => expect(mocks.interruptTurn).toHaveBeenCalledWith(SIDE, "turn-9"));
  });

  it("does not stay busy on a stale in-progress turn from a dead session", async () => {
    mocks.readThread.mockResolvedValue(
      detail([
        {
          id: "turn-old",
          status: "inProgress",
          items: [{ type: "userMessage", id: "u1", content: [{ type: "text", text: "Earlier" }] }],
        },
      ]),
    );
    setup();

    expect(await screen.findByText("Earlier")).toBeInTheDocument();
    expect(stopButton()).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Codex is working")).not.toBeInTheDocument();
  });

  it("keeps a turn running when the active-turn store says the thread is working", async () => {
    activeTurns.list = [SIDE];
    mocks.readThread.mockResolvedValue(detail([{ id: "turn-live", status: "inProgress", items: [] }]));
    setup();

    expect(await screen.findByLabelText("Codex is working")).toBeInTheDocument();
    expect(mocks.invalidateThreadCache).toHaveBeenCalledWith(SIDE);
  });

  it("unlocks the composer when Codex closes the side thread", async () => {
    const user = userEvent.setup();
    setup();
    await user.type(await screen.findByPlaceholderText("Follow up…"), "Why?{Enter}");
    await screen.findByLabelText("Codex is working");

    previewEmit({ method: "thread/closed", params: { threadId: SIDE } });
    await waitFor(() => expect(stopButton()).not.toBeInTheDocument());
  });
});
