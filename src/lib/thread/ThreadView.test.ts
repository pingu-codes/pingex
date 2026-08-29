import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { appData, trackNewThread } from "$lib/app/appData.svelte";
import { loadPrefs, savePrefs } from "$lib/composer/composerPrefs.svelte";
import type { ThreadDetail, Turn } from "$lib/types";

type ThreadEventHandler = (event: { method: string; params: Record<string, unknown> }) => void;

const mocks = vi.hoisted(() => ({
  readThread: vi.fn(),
  invalidateThreadCache: vi.fn().mockResolvedValue(undefined),
  listProjectFiles: vi.fn(),
  rollbackThread: vi.fn(),
  revertThread: vi.fn(),
  openDialog: vi.fn(),
  startThread: vi.fn(),
  startTurn: vi.fn(),
  compactThread: vi.fn(),
  gitRepoInfo: vi.fn(),
  gitRecentCommits: vi.fn(),
  gitBranches: vi.fn(),
  startReview: vi.fn(),
  interruptTurn: vi.fn(),
  setThreadGoal: vi.fn(),
  getThreadGoal: vi.fn(),
  clearThreadGoal: vi.fn(),
  queueAdd: vi.fn(),
  queueDelete: vi.fn(),
  queueList: vi.fn(),
  queueUpdate: vi.fn(),
  queueReorder: vi.fn(),
  handlers: [] as ThreadEventHandler[],
  setThreadHandler: vi.fn((handler: ThreadEventHandler) => {
    mocks.handlers.push(handler);
    return () => {
      mocks.handlers = mocks.handlers.filter((candidate) => candidate !== handler);
    };
  }),
  requestAutoName: vi.fn(),
  toastError: vi.fn(),
  // The default is a live session that owns thread-1's turn; emptying it makes
  // a loaded in-progress turn look like one a dead session left behind.
  activeTurns: { list: ["thread-1"] },
}));

vi.mock("$lib/thread/autoName", () => ({
  requestAutoName: mocks.requestAutoName,
}));

// Errors leave this view as toasts, and the toast host lives in App — outside
// this tree. Stub it so the messages are assertable here.
vi.mock("$lib/toaster", () => ({
  toaster: {},
  toastError: mocks.toastError,
}));

vi.mock("$lib/services/api", () => ({
  addSideQuestion: vi.fn(),
  compactThread: mocks.compactThread,
  // The composer persists what is typed on a debounce; without these the timer
  // fires into `undefined` and takes the worker down with it.
  loadDraft: vi.fn().mockResolvedValue(null),
  saveDraft: vi.fn().mockResolvedValue(undefined),
  deleteDraft: vi.fn().mockResolvedValue(undefined),
  forkThread: vi.fn(),
  gitRepoInfo: mocks.gitRepoInfo,
  gitRecentCommits: mocks.gitRecentCommits,
  gitBranches: mocks.gitBranches,
  startReview: mocks.startReview,
  interruptTurn: mocks.interruptTurn,
  setThreadGoal: mocks.setThreadGoal,
  getThreadGoal: mocks.getThreadGoal,
  setThreadGoalStatus: vi.fn(),
  clearThreadGoal: mocks.clearThreadGoal,
  invalidateThreadCache: mocks.invalidateThreadCache,
  isTauri: () => false,
  listModels: vi.fn().mockResolvedValue([
    {
      id: "gpt-5.2-codex",
      model: "gpt-5.2-codex",
      displayName: "GPT-5.2 Codex",
      description: "",
      hidden: false,
      supportedReasoningEfforts: [{ reasoningEffort: "high", description: "" }],
      defaultReasoningEffort: "high",
      isDefault: true,
    },
  ]),
  listProjectFiles: mocks.listProjectFiles,
  listSubagents: vi.fn().mockResolvedValue([]),
  listAgentRuns: vi.fn().mockResolvedValue([]),
  killAgentRun: vi.fn().mockResolvedValue(undefined),
  openInZed: vi.fn(),
  isQueueUnsupported: (cause: unknown) =>
    (cause instanceof Error ? cause.message : String(cause)).startsWith("codex-queue-unsupported"),
  isRevertUnsupported: (cause: unknown) =>
    (cause instanceof Error ? cause.message : String(cause)).startsWith("codex-revert-unsupported"),
  queueAdd: mocks.queueAdd,
  queueDelete: mocks.queueDelete,
  queueList: mocks.queueList,
  queueUpdate: mocks.queueUpdate,
  queueReorder: mocks.queueReorder,
  readThread: mocks.readThread,
  removeSideQuestion: vi.fn(),
  respondUserInput: vi.fn(),
  revealInFinder: vi.fn(),
  revertThread: mocks.revertThread,
  rollbackThread: mocks.rollbackThread,
  startThread: mocks.startThread,
  startTurn: mocks.startTurn,
  updateSubagentPolicy: vi.fn(),
}));

// Only `openDialog` is stubbed: other consumers in this tree (worktree dialogs)
// use the real `submitState`.
vi.mock("$lib/app/dialogs.svelte", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/app/dialogs.svelte")>()),
  openDialog: mocks.openDialog,
}));

vi.mock("$lib/services/codexEvents.svelte", () => ({
  activeTurns: mocks.activeTurns,
  approvals: { list: [] },
  userInputRequests: { list: [] },
  elicitations: { list: [] },
  threadTokenUsage: {},
  turnPlans: { byThread: {} },
  setThreadHandler: mocks.setThreadHandler,
  clearUnanswered: vi.fn(),
  removeUserInputRequest: vi.fn(),
}));

import { resetSessions } from "$lib/thread/sessions.svelte";
import ThreadView from "$lib/thread/ThreadView.svelte";

function detail(...turns: Turn[]): ThreadDetail {
  return {
    id: "thread-1",
    preview: "Preview",
    cwd: "/projects/example",
    turns,
  };
}

function completedTurn(overrides: Partial<Turn> = {}): Turn {
  return {
    id: "turn-1",
    status: "completed",
    durationMs: 24_000,
    items: [
      { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] },
      { id: "reasoning-1", type: "reasoning", summary: ["Private work summary"] },
      { id: "answer-1", type: "agentMessage", text: "Final answer" },
    ],
    ...overrides,
  };
}

async function renderTurn(turn: Turn) {
  mocks.readThread.mockResolvedValueOnce(detail(turn));
  render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });
  return screen.findByRole("button", { name: /Worked/ });
}

/**
 * The composer refuses to send without an explicit model and permission preset,
 * so anything driving it through a send needs a valid pair on hand.
 */
function seedComposerPrefs() {
  savePrefs({ ...loadPrefs(), model: "gpt-5.2-codex", permissionPreset: "auto" });
}

beforeEach(() => {
  seedComposerPrefs();
  mocks.toastError.mockReset();
  mocks.queueAdd.mockReset();
  mocks.queueDelete.mockReset();
  mocks.queueList.mockReset();
  // A working server queue by default; the tests that care override these.
  mocks.queueAdd.mockImplementation((_threadId, input, clientUserMessageId) =>
    Promise.resolve({ id: `q-${clientUserMessageId}`, input, clientUserMessageId }),
  );
  mocks.queueDelete.mockResolvedValue(true);
  mocks.queueList.mockResolvedValue([]);
  // Opening a thread reads its goal; no goal unless a test sets one.
  mocks.getThreadGoal.mockReset();
  mocks.getThreadGoal.mockResolvedValue(null);
});

describe("ThreadView completed work", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
  });

  it("keeps work collapsed until expanded while leaving the final answer visible", async () => {
    const user = userEvent.setup();
    const trigger = await renderTurn(completedTurn());

    expect(trigger).toHaveAccessibleName("Worked for 24s");
    expect(screen.getByText("Final answer")).toBeVisible();
    expect(screen.getByText("Private work summary")).not.toBeVisible();

    await user.click(trigger);
    expect(screen.getByText("Private work summary")).toBeVisible();
  });

  it("labels a message with the model and effort its turn ran on", async () => {
    await renderTurn(completedTurn({ model: "gpt-5.2-codex", reasoningEffort: "high" }));

    expect(await screen.findByText(/GPT-5\.2 Codex · high/)).toBeInTheDocument();
  });

  it("leaves a turn that predates per-turn settings unlabelled", async () => {
    await renderTurn(completedTurn());

    expect(screen.queryByText(/Default model/)).not.toBeInTheDocument();
  });

  it("keeps preamble agent messages visible instead of collapsing them into work", async () => {
    const trigger = await renderTurn(
      completedTurn({
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] },
          { id: "preamble-1", type: "agentMessage", text: "I'll ground this in the current router test" },
          { id: "reasoning-1", type: "reasoning", summary: ["Private work summary"] },
          { id: "answer-1", type: "agentMessage", text: "Final answer" },
        ],
      }),
    );

    expect(screen.getByText("I'll ground this in the current router test")).toBeVisible();
    expect(screen.getByText("Final answer")).toBeVisible();
    expect(screen.getByText("Private work summary")).not.toBeVisible();
    expect(trigger).toHaveAccessibleName("Worked for 24s");
  });

  it.each([
    [completedTurn({ durationMs: 84 }), "Worked for 84ms"],
    [completedTurn({ durationMs: 24_000 }), "Worked for 24s"],
    [completedTurn({ durationMs: null, startedAt: 10, completedAt: 75.4 }), "Worked for 1m 05s"],
    [completedTurn({ durationMs: null, startedAt: null, completedAt: null }), "Worked"],
  ])("formats completed timing as %s", async (turn, label) => {
    const trigger = await renderTurn(turn);
    expect(trigger).toHaveAccessibleName(label);
  });
});

describe("ThreadView in-progress work", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
  });

  it("collapses work before the latest message while keeping the live tail visible", async () => {
    const trigger = await renderTurn({
      id: "turn-1",
      status: "inProgress",
      items: [
        { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] },
        { id: "reasoning-1", type: "reasoning", summary: ["Earlier work summary"] },
        { id: "cmd-1", type: "commandExecution", command: "cargo fmt", status: "completed", exitCode: 0 },
        { id: "preamble-1", type: "agentMessage", text: "Now running tests" },
        { id: "cmd-2", type: "commandExecution", command: "cargo test", status: "inProgress" },
      ],
    });

    expect(trigger).toHaveAccessibleName("Worked");
    expect(screen.getByText("Earlier work summary")).not.toBeVisible();
    expect(screen.getByText("cargo fmt")).not.toBeVisible();
    expect(screen.getByText("Now running tests")).toBeVisible();
    expect(screen.getByText("cargo test")).toBeVisible();
  });

  it("auto-collapses diffs once a turn touches more than one file", async () => {
    mocks.readThread.mockResolvedValueOnce(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Change files" }] },
          {
            id: "change-1",
            type: "fileChange",
            status: "completed",
            changes: [
              { path: "src/a.ts", kind: { type: "update" }, diff: "+const a = 1;" },
              { path: "src/b.ts", kind: { type: "update" }, diff: "+const b = 2;" },
            ],
          },
        ],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    expect(await screen.findByText("src/a.ts")).toBeVisible();
    expect(screen.getByText("+const a = 1;")).not.toBeVisible();
    expect(screen.getByText("+const b = 2;")).not.toBeVisible();
  });

  it("keeps showing work after a preamble message finishes streaming", async () => {
    mocks.readThread.mockResolvedValueOnce(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] },
          { id: "preamble-1", type: "agentMessage", text: "I'm adding x to y" },
        ],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    expect(await screen.findByText("I'm adding x to y")).toBeVisible();
    expect(screen.getByLabelText("Codex is working")).toBeVisible();
  });

  it("hides the dots while the message itself is still streaming in", async () => {
    mocks.readThread.mockResolvedValueOnce(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] },
          { id: "preamble-1", type: "agentMessage", text: "I'm adding x", streaming: true },
        ],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    expect(await screen.findByText("I'm adding x")).toBeVisible();
    expect(screen.queryByLabelText("Codex is working")).not.toBeInTheDocument();
  });

  it("lists created and edited files together in Outputs and in the Changes panel", async () => {
    const user = userEvent.setup();
    mocks.readThread.mockResolvedValueOnce(
      detail({
        id: "turn-1",
        status: "completed",
        durationMs: 1000,
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Change files" }] },
          {
            id: "change-1",
            type: "fileChange",
            status: "completed",
            changes: [{ path: "src/new.ts", kind: { type: "add" }, diff: "+created" }],
          },
          {
            id: "change-2",
            type: "fileChange",
            status: "completed",
            changes: [{ path: "src/existing.ts", kind: { type: "update" }, diff: "+edited" }],
          },
        ],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    // Outputs is the thread's inventory of touched files: a file it edited
    // belongs there just as much as one it created.
    expect(await screen.findByTitle("View diff for src/new.ts")).toHaveTextContent(/new\.ts\s*New/);
    expect(screen.getByTitle("View diff for src/existing.ts")).toHaveTextContent(/existing\.ts\s*Edited/);

    await user.click(screen.getByText("All 2 changed files"));
    const panel = screen.getByRole("complementary", { name: "Thread side panel" });
    expect(within(panel).getByText("src/new.ts")).toBeInTheDocument();
    expect(within(panel).getByText("src/existing.ts")).toBeInTheDocument();
  });

  it("marks a started file change as in flight before its patch arrives", async () => {
    mocks.readThread.mockResolvedValueOnce(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Change files" }] },
          { id: "change-1", type: "fileChange", status: "inProgress", changes: [] },
        ],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    expect(await screen.findByText("Editing files…")).toBeVisible();
  });
});

describe("ThreadView questions stranded by an earlier session", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.startTurn.mockReset();
    mocks.activeTurns.list = [];
  });

  const strandedTurn = (): Turn => ({
    id: "turn-1",
    status: "inProgress",
    items: [
      { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Ship it" }] },
      {
        id: "item_2",
        type: "userInputAnswered",
        unanswered: true,
        questions: [{ id: "q1", header: "Target", question: "Which environment?" }],
        answers: {},
      },
    ],
  });

  it("sends the answer as a new message, since the original request is gone", async () => {
    const user = userEvent.setup();
    mocks.readThread.mockResolvedValueOnce(detail(strandedTurn()));
    mocks.startTurn.mockResolvedValue({ id: "turn-2", status: "inProgress" });
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    expect(await screen.findByText("Codex asked this before the app closed")).toBeVisible();
    await user.type(screen.getByPlaceholderText("Answer…"), "staging");
    await user.click(screen.getByRole("button", { name: "Send as new message" }));

    expect(mocks.startTurn).toHaveBeenCalledWith(
      "thread-1",
      [{ type: "text", text: "Which environment?\nstaging" }],
      undefined,
    );
  });

  it("does not leave the dead turn looking like it is still working", async () => {
    mocks.readThread.mockResolvedValueOnce(detail(strandedTurn()));
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    await screen.findByText("Codex asked this before the app closed");
    expect(screen.queryByLabelText("Codex is working")).not.toBeInTheDocument();
  });
});

describe("ThreadView file references", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
  });

  it("restores mention chips from the markdown links Codex persists", async () => {
    mocks.readThread.mockResolvedValueOnce(
      detail(
        completedTurn({
          items: [
            {
              id: "user-1",
              type: "userMessage",
              content: [{ type: "text", text: "explain [utils.ts](src/lib/utils.ts) to me" }],
            },
            { id: "answer-1", type: "agentMessage", text: "Final answer" },
          ],
        }),
      ),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    const chip = await screen.findByRole("button", { name: "@utils.ts" });
    expect(chip).toHaveAttribute("title", "/projects/example/src/lib/utils.ts");
    expect(screen.getByText("explain", { exact: false })).toBeVisible();
  });
});

describe("ThreadView inline message editing", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.rollbackThread.mockReset();
    mocks.revertThread.mockReset();
    // The default stand-in is a codex CLI too old for `thread/revert`, so the
    // rewind goes through the deprecated rollback API.
    mocks.revertThread.mockRejectedValue(new Error("codex-revert-unsupported: too old"));
    mocks.startTurn.mockReset();
    mocks.openDialog.mockReset();
    mocks.openDialog.mockResolvedValue(true);
  });

  async function edit(text: string) {
    const user = userEvent.setup();
    await renderTurn(completedTurn());

    await user.click(screen.getByRole("button", { name: "Edit and resend" }));
    const editor = screen.getByRole("textbox", { name: "Edit message" });
    expect(editor).toHaveValue("Do the work");

    await user.clear(editor);
    await user.type(editor, text);
    await user.click(screen.getByRole("button", { name: "Send" }));
  }

  it("rewinds the same thread to the edited turn and resends on it", async () => {
    mocks.rollbackThread.mockResolvedValue({ id: "thread-1" });
    mocks.startTurn.mockResolvedValue({ id: "turn-2", status: "inProgress" });

    await edit("Do different work");

    // One turn dropped: the edited one, which is the last in the transcript.
    expect(mocks.rollbackThread).toHaveBeenCalledWith("thread-1", 1);
    expect(mocks.startTurn).toHaveBeenCalledWith("thread-1", [{ type: "text", text: "Do different work" }], undefined);
    // The rewound turn is gone and the edited message is visible immediately.
    expect(screen.queryByText("Final answer")).not.toBeInTheDocument();
    expect(screen.getByText("Do different work")).toBeVisible();
  });

  it("leaves the thread untouched when the confirmation is dismissed", async () => {
    mocks.openDialog.mockResolvedValue(null);

    await edit("Do different work");

    expect(mocks.rollbackThread).not.toHaveBeenCalled();
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(screen.getByText("Final answer")).toBeVisible();
  });

  it("reports a failed rewind without dropping history or resending", async () => {
    mocks.rollbackThread.mockRejectedValue(new Error("Rollback unsupported"));

    await edit("Do different work");

    await vi.waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith("Rollback unsupported"));
    expect(mocks.startTurn).not.toHaveBeenCalled();
    expect(screen.getByText("Do the work")).toBeVisible();
    expect(screen.getByText("Final answer")).toBeVisible();
  });

  it("rewinds with thread/revert on a codex that has it", async () => {
    mocks.revertThread.mockResolvedValue({ thread: { id: "thread-1", turns: [] } });
    mocks.startTurn.mockResolvedValue({ id: "turn-2", status: "inProgress" });

    await edit("Do different work");

    expect(mocks.revertThread).toHaveBeenCalledWith("thread-1", "turn-1", []);
    expect(mocks.rollbackThread).not.toHaveBeenCalled();
    expect(mocks.startTurn).toHaveBeenCalled();
  });

  it("does not fall back to rollback when revert fails for a real reason", async () => {
    mocks.revertThread.mockRejectedValue(new Error("thread not found"));

    await edit("Do different work");

    await vi.waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith("thread not found"));
    expect(mocks.rollbackThread).not.toHaveBeenCalled();
    expect(mocks.startTurn).not.toHaveBeenCalled();
  });

  it("cancels an in-place edit with Escape without rewinding", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());

    await user.click(screen.getByRole("button", { name: "Edit and resend" }));
    await user.keyboard("{Escape}");

    expect(screen.queryByRole("textbox", { name: "Edit message" })).not.toBeInTheDocument();
    expect(screen.getByText("Do the work")).toBeVisible();
    expect(mocks.openDialog).not.toHaveBeenCalled();
    expect(mocks.rollbackThread).not.toHaveBeenCalled();
  });
});

describe("ThreadView right panel", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.listProjectFiles.mockReset();
  });

  function turnWithChanges(): Turn {
    return completedTurn({
      items: [
        { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Change some files" }] },
        {
          id: "change-1",
          type: "fileChange",
          status: "completed",
          changes: [
            { path: "src/lib/api.ts", kind: { type: "update" }, diff: "+const added = 1;" },
            { path: "docs/notes.md", kind: { type: "add" }, diff: "+# Notes" },
          ],
        },
        { id: "answer-1", type: "agentMessage", text: "Final answer" },
      ],
    });
  }

  it("opens the changes panel focused on an output clicked in the overview menu", async () => {
    const user = userEvent.setup();
    await renderTurn(turnWithChanges());

    await user.click(screen.getByTitle("View diff for src/lib/api.ts"));

    const panel = screen.getByRole("complementary", { name: "Thread side panel" });
    expect(within(panel).getByText("Outputs")).toBeInTheDocument();
    expect(within(panel).getByText("src/lib/api.ts")).toBeInTheDocument();
    expect(within(panel).getByText("docs/notes.md")).toBeInTheDocument();
    expect(within(panel).getByText("+const added = 1;")).toBeVisible();

    await user.click(within(panel).getByRole("button", { name: "Close panel" }));
    expect(screen.queryByRole("complementary", { name: "Thread side panel" })).not.toBeInTheDocument();
  });

  it("opens the file tree panel from the Files button", async () => {
    const user = userEvent.setup();
    mocks.listProjectFiles.mockResolvedValue(["docs/guide.md", "main.ts"]);
    await renderTurn(completedTurn());

    await user.click(screen.getByRole("button", { name: "Files" }));

    const panel = screen.getByRole("complementary", { name: "Thread side panel" });
    expect(mocks.listProjectFiles).toHaveBeenCalledWith("/projects/example");
    expect(await within(panel).findByText("main.ts")).toBeInTheDocument();
    expect(within(panel).queryByText("guide.md")).not.toBeInTheDocument();
    await user.click(within(panel).getByText("docs"));
    expect(within(panel).getByText("guide.md")).toBeInTheDocument();
  });
});

describe("ThreadView context meter and compaction", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.compactThread.mockReset();
    mocks.compactThread.mockResolvedValue(undefined);
    mocks.setThreadHandler.mockClear();
  });

  function emitTokenUsage(lastTotal: number, contextWindow: number) {
    const breakdown = (totalTokens: number) => ({
      totalTokens,
      inputTokens: totalTokens - 1_000,
      cachedInputTokens: 500,
      cacheWriteInputTokens: 0,
      outputTokens: 1_000,
      reasoningOutputTokens: 400,
    });
    for (const [handler] of mocks.setThreadHandler.mock.calls) {
      handler({
        method: "thread/tokenUsage/updated",
        params: {
          threadId: "thread-1",
          turnId: "turn-1",
          tokenUsage: {
            total: breakdown(lastTotal * 3),
            last: breakdown(lastTotal),
            modelContextWindow: contextWindow,
          },
        },
      });
    }
  }

  it("stays hidden until Codex reports token usage, then shows the used share", async () => {
    await renderTurn(completedTurn());
    expect(screen.queryByRole("button", { name: /^Context:/ })).not.toBeInTheDocument();

    // 12k baseline plus a quarter of the 100k usable window.
    emitTokenUsage(37_000, 112_000);
    expect(await screen.findByRole("button", { name: "Context: 25% used" })).toBeInTheDocument();
  });

  it("reveals the exact token stats on hover", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());
    emitTokenUsage(37_000, 112_000);

    const meter = await screen.findByRole("button", { name: "Context: 25% used" });
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();

    await user.hover(meter);
    const tooltip = screen.getByRole("tooltip");
    expect(within(tooltip).getByText("37,000")).toBeInTheDocument();
    expect(within(tooltip).getByText("112,000")).toBeInTheDocument();
    expect(within(tooltip).getByText("75%")).toBeInTheDocument();
    expect(within(tooltip).getByText("111,000")).toBeInTheDocument();
  });

  it("compacts the live thread when the meter is clicked", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());
    emitTokenUsage(100_000, 112_000);

    await user.click(await screen.findByRole("button", { name: /^Context:/ }));
    expect(mocks.compactThread).toHaveBeenCalledWith("thread-1");
  });

  it("renders a compaction marker outside the collapsed work section", async () => {
    await renderTurn(
      completedTurn({
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] },
          { id: "reasoning-1", type: "reasoning", summary: ["Private work summary"] },
          { id: "compaction-1", type: "contextCompaction" },
          { id: "answer-1", type: "agentMessage", text: "Final answer" },
        ],
      }),
    );

    expect(screen.getByText("Context compacted")).toBeVisible();
    expect(screen.getByText("Private work summary")).not.toBeVisible();
  });
});

describe("ThreadView plan handoff", () => {
  beforeEach(() => {
    localStorage.clear();
    seedComposerPrefs();
    mocks.readThread.mockReset();
    mocks.startThread.mockReset();
    mocks.startTurn.mockReset();
    mocks.startThread.mockResolvedValue({ id: "thread-2", cwd: "/projects/example" });
    mocks.startTurn.mockResolvedValue({ id: "turn-2", status: "inProgress" });
  });

  it("implements a plan in a brand-new thread on the same directory", async () => {
    const user = userEvent.setup();
    const onThreadCreated = vi.fn();
    mocks.readThread.mockResolvedValueOnce(
      detail({
        id: "turn-1",
        status: "completed",
        items: [
          { id: "user-1", type: "userMessage", content: [{ type: "text", text: "Plan it" }] },
          { id: "plan-1", type: "plan", text: "1. Rewire the composer" },
        ],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example", onThreadCreated });
    await screen.findByText("1. Rewire the composer");

    await user.click(screen.getByRole("button", { name: "Toggle plan mode" }));
    await user.click(screen.getByRole("button", { name: "Clear context and implement the plan" }));

    // The third argument is the per-thread agent-tools choice; null follows the
    // global setting.
    expect(mocks.startThread).toHaveBeenCalledWith("/projects/example", null, null, null);
    const [threadId, input] = mocks.startTurn.mock.calls[0];
    expect(threadId).toBe("thread-2");
    expect(input[0].text).toContain("1. Rewire the composer");
    expect(onThreadCreated).toHaveBeenCalledWith("thread-2", "/projects/example");
  });
});

describe("ThreadView workspace starts", () => {
  beforeEach(() => {
    mocks.startThread.mockReset();
    mocks.startTurn.mockReset();
    mocks.gitRepoInfo.mockResolvedValue({ isGitRepo: true, root: "/workspace/hub" });
    mocks.gitRecentCommits.mockResolvedValue([]);
    mocks.startThread.mockResolvedValue({ id: "workspace-thread", cwd: "/workspace/hub" });
    mocks.startTurn.mockResolvedValue({ id: "turn-1", status: "inProgress" });
  });

  it("starts new workspace threads with the workspace id and keeps the hub fixed", async () => {
    const user = userEvent.setup();
    render(ThreadView, {
      threadId: null,
      cwd: "/workspace/hub",
      projectPath: "/workspace/hub",
      workspaceId: "workspace-1",
    });
    const message = screen.getByRole("textbox", { name: "Message Codex… (@ to attach files, / for commands)" });

    expect(screen.getByText("Workspace hub — shared notes and all member roots are writable")).toBeVisible();
    expect(screen.queryByRole("button", { name: "Temporary worktree" })).not.toBeInTheDocument();
    await user.type(message, "Coordinate the release{Enter}");

    expect(mocks.startThread).toHaveBeenCalledWith("/workspace/hub", "workspace-1", null, null);
    // No per-turn overrides beyond the resolved pair the transcript labels
    // replies with and the collaboration mode every turn restates.
    expect(mocks.startTurn).toHaveBeenCalledWith(
      "workspace-thread",
      [{ type: "text", text: "Coordinate the release" }],
      {
        model: "gpt-5.2-codex",
        approvalPolicy: "on-request",
        sandboxMode: "workspace-write",
        collaborationMode: {
          mode: "default",
          settings: { model: "gpt-5.2-codex", reasoning_effort: null, developer_instructions: null },
        },
        resolvedModel: "gpt-5.2-codex",
        resolvedEffort: "high",
      },
    );
  });
});

describe("ThreadView thread naming", () => {
  beforeEach(() => {
    resetSessions();
    mocks.handlers = [];
    mocks.readThread.mockReset();
    mocks.startThread.mockReset();
    mocks.startTurn.mockReset();
    mocks.requestAutoName.mockReset();
    mocks.startThread.mockResolvedValue({ id: "thread-2", cwd: "/projects/example" });
    mocks.startTurn.mockResolvedValue({ id: "turn-2", status: "inProgress" });
  });

  const emit = (method: string, params: Record<string, unknown>) => {
    for (const handler of [...mocks.handlers]) handler({ method, params });
  };

  it("names a brand-new thread from its opening message", async () => {
    const user = userEvent.setup();
    render(ThreadView, { threadId: null, cwd: "/projects/example" });

    await user.type(
      screen.getByRole("textbox", { name: "Message Codex… (@ to attach files, / for commands)" }),
      "Refactor the sidebar grouping{Enter}",
    );

    expect(mocks.requestAutoName).toHaveBeenCalledWith("thread-2", "seed", "Refactor the sidebar grouping");
  });

  it("titles the sidebar entry without waiting for the turn to start", async () => {
    // A turn that never starts stands in for the seconds one normally takes:
    // the title must not be held back by it.
    mocks.startTurn.mockReturnValue(new Promise(() => {}));
    appData.data = {
      codexHome: "/home/.codex",
      codexBinary: "codex",
      projects: [
        {
          name: "example",
          path: "/projects/example",
          kind: "folder",
          workspaceId: null,
          archived: false,
          instructions: "",
          sources: [],
          pinned: false,
          expanded: true,
          threads: [],
        },
      ],
      account: null,
      sideQuestions: [],
      subagents: [],
      sections: [],
      sectionsSupported: false,
      sidebarLayout: { folders: [], placements: [] },
    };
    const user = userEvent.setup();
    render(ThreadView, {
      threadId: null,
      cwd: "/projects/example",
      // What the app shell does with a draft that has become a real thread.
      onThreadCreated: (id: string, cwd: string) => trackNewThread(id, cwd),
    });

    await user.type(
      screen.getByRole("textbox", { name: "Message Codex… (@ to attach files, / for commands)" }),
      "Refactor the sidebar grouping{Enter}",
    );

    await waitFor(() => {
      expect(appData.data?.projects[0].threads[0]?.title).toBe("Refactor the sidebar grouping");
    });
    expect(mocks.requestAutoName).toHaveBeenCalledWith("thread-2", "seed", "Refactor the sidebar grouping");
  });

  it("re-names off the exchange once the opening turn completes, and only then", async () => {
    mocks.readThread.mockResolvedValue(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [{ id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] }],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });
    await screen.findByText("Do the work");
    expect(mocks.requestAutoName).not.toHaveBeenCalled();

    emit("turn/completed", { threadId: "thread-1", turn: { id: "turn-1", status: "completed" } });
    expect(mocks.requestAutoName).toHaveBeenCalledWith("thread-1", "reply");
  });

  it("leaves an established thread's title alone as later turns complete", async () => {
    mocks.readThread.mockResolvedValue(
      detail({ id: "turn-1", status: "completed", items: [] }, { id: "turn-2", status: "inProgress", items: [] }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });
    await screen.findByRole("textbox", { name: "Message Codex… (@ to attach files, / for commands)" });

    emit("turn/completed", { threadId: "thread-1", turn: { id: "turn-2", status: "completed" } });
    expect(mocks.requestAutoName).not.toHaveBeenCalled();
  });
});

describe("ThreadView switching between working threads", () => {
  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.handlers = [];
    mocks.activeTurns.list = ["thread-1"];
  });

  const emit = (method: string, params: Record<string, unknown>) => {
    for (const handler of [...mocks.handlers]) handler({ method, params });
  };

  it("keeps streaming into a thread left mid-turn and shows it again on return", async () => {
    mocks.readThread.mockResolvedValue(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [{ id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] }],
      }),
    );
    const first = render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });
    await screen.findByText("Do the work");

    // Switch to another thread: this view goes away, the turn does not.
    first.unmount();
    emit("item/agentMessage/delta", { threadId: "thread-1", turnId: "turn-1", itemId: "msg-1", delta: "Half done" });

    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });

    expect(await screen.findByText("Half done")).toBeVisible();
    // Still mid-turn, so the composer keeps offering Stop rather than Send.
    expect(screen.getByRole("button", { name: "Stop" })).toBeInTheDocument();
    // The retained transcript is used as-is; nothing is re-read behind it.
    expect(mocks.readThread).toHaveBeenCalledTimes(1);
  });

  it("drops the stale detail cache before reading a thread that is mid-turn", async () => {
    mocks.readThread.mockResolvedValue(detail(completedTurn()));
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });
    await screen.findByText("Final answer");

    expect(mocks.invalidateThreadCache).toHaveBeenCalledWith("thread-1");
  });
});

// The queue's own behaviour (drain order, retries, merging server listings) is
// covered in threadQueue.test.ts; these check the view is wired to it.
describe("ThreadView queueing when Codex cannot hold the queue", () => {
  const composerLabel = "Message Codex… (@ to attach files, / for commands)";
  const unsupported = new Error("codex-queue-unsupported: this Codex version is older than the thread/queue APIs");

  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.startTurn.mockReset();
    mocks.handlers = [];
    mocks.activeTurns.list = ["thread-1"];
    mocks.startTurn.mockResolvedValue({ id: "turn-2", status: "inProgress" });
  });

  const emit = (method: string, params: Record<string, unknown>) => {
    for (const handler of [...mocks.handlers]) handler({ method, params });
  };

  /** End the live turn, which is what lets the queue drain. */
  function finishTurn() {
    mocks.activeTurns.list = [];
    emit("turn/completed", { threadId: "thread-1", turn: { id: "turn-1", status: "completed" } });
  }

  /** Render thread-1 mid-turn, so anything typed is queued rather than sent. */
  async function renderWorking() {
    mocks.readThread.mockResolvedValue(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [{ id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] }],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });
    return screen.findByRole("button", { name: "Stop" });
  }

  it("keeps the message and says it is only held here", async () => {
    const user = userEvent.setup();
    mocks.queueAdd.mockRejectedValue(unsupported);
    await renderWorking();

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "Then do this{Enter}");

    expect(await screen.findByText("Queued locally")).toBeVisible();
    expect(screen.getByText("Then do this")).toBeVisible();
    // An old Codex is not an error, so nothing red and nothing to read.
    expect(screen.queryByText(/codex-queue-unsupported/)).not.toBeInTheDocument();
  });

  it("edits a queued message in place", async () => {
    const user = userEvent.setup();
    mocks.queueAdd.mockRejectedValue(unsupported);
    await renderWorking();
    await user.type(screen.getByRole("textbox", { name: composerLabel }), "Then do this{Enter}");
    await screen.findByText("Queued locally");

    await user.click(screen.getByRole("button", { name: "Edit queued message" }));
    const field = screen.getByRole("textbox", { name: "Edit queued message" });
    await user.clear(field);
    await user.type(field, "Do that instead{Enter}");

    expect(await screen.findByText("Do that instead")).toBeVisible();
    finishTurn();
    await vi.waitFor(() => expect(mocks.startTurn).toHaveBeenCalled());
    expect(mocks.startTurn.mock.calls[0][1]).toEqual([{ type: "text", text: "Do that instead" }]);
  });

  it("cancel moves the message back into an empty composer", async () => {
    const user = userEvent.setup();
    mocks.queueAdd.mockRejectedValue(unsupported);
    mocks.openDialog.mockReset();
    await renderWorking();
    const composer = screen.getByRole("textbox", { name: composerLabel });
    await user.type(composer, "Then do this{Enter}");
    await screen.findByText("Queued locally");

    await user.click(screen.getByRole("button", { name: "Remove queued message" }));

    await vi.waitFor(() => expect(composer.textContent).toBe("Then do this"));
    expect(screen.queryByText("Queued locally")).not.toBeInTheDocument();
    expect(mocks.openDialog).not.toHaveBeenCalled();
  });

  it("cancel asks before discarding when the composer holds text", async () => {
    const user = userEvent.setup();
    mocks.queueAdd.mockRejectedValue(unsupported);
    mocks.openDialog.mockReset();
    mocks.openDialog.mockResolvedValue(null);
    await renderWorking();
    const composer = screen.getByRole("textbox", { name: composerLabel });
    await user.type(composer, "Then do this{Enter}");
    await screen.findByText("Queued locally");
    await user.type(composer, "unsent");

    await user.click(screen.getByRole("button", { name: "Remove queued message" }));
    await vi.waitFor(() => expect(mocks.openDialog).toHaveBeenCalled());
    expect(screen.getByText("Queued locally")).toBeVisible();

    mocks.openDialog.mockResolvedValue(true);
    await user.click(screen.getByRole("button", { name: "Remove queued message" }));
    await vi.waitFor(() => expect(screen.queryByText("Queued locally")).not.toBeInTheDocument());
    expect(composer.textContent).toBe("unsent");
  });

  it("explains a queue that exists but refused, without losing the message", async () => {
    const user = userEvent.setup();
    mocks.queueAdd.mockRejectedValue(new Error("queue cannot contain more than 100 submissions"));
    await renderWorking();

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "One too many{Enter}");

    expect(await screen.findByText(/queue cannot contain more than 100 submissions/)).toBeVisible();
    expect(screen.getByText("Queued locally")).toBeVisible();
    expect(screen.getByText("One too many")).toBeVisible();
  });
});

describe("ThreadView /review", () => {
  const composerLabel = "Message Codex… (@ to attach files, / for commands)";

  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.startThread.mockReset();
    mocks.startReview.mockReset();
    mocks.startReview.mockResolvedValue({ id: "review-turn-1", status: "inProgress", items: [] });
    mocks.interruptTurn.mockReset();
    mocks.interruptTurn.mockResolvedValue(undefined);
    mocks.gitBranches.mockReset();
    mocks.gitBranches.mockResolvedValue([
      { name: "main", isRemote: false, isCurrent: true },
      { name: "origin/main", isRemote: true, isCurrent: false },
    ]);
    mocks.gitRepoInfo.mockResolvedValue({ isGitRepo: true, root: "/projects/example" });
    mocks.gitRecentCommits.mockResolvedValue([]);
    mocks.startThread.mockResolvedValue({ id: "thread-2", cwd: "/projects/example" });
  });

  it("asks what to review and starts the chosen target", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "/review{Enter}");
    await user.click(await screen.findByRole("option", { name: "Review uncommitted changes" }));

    expect(mocks.startReview).toHaveBeenCalledWith("thread-1", { type: "uncommittedChanges" });
  });

  it("reviews against a base branch chosen from the repository", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "/review{Enter}");
    await user.click(await screen.findByRole("option", { name: "Review against a base branch" }));
    await user.click(await screen.findByRole("option", { name: /origin\/main/ }));

    expect(mocks.gitBranches).toHaveBeenCalledWith("/projects/example");
    expect(mocks.startReview).toHaveBeenCalledWith("thread-1", {
      type: "baseBranch",
      branch: "origin/main",
    });
  });

  it("starts a draft thread rather than dropping the command", async () => {
    const user = userEvent.setup();
    render(ThreadView, { threadId: null, cwd: "/projects/example", projectPath: "/projects/example" });

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "/review{Enter}");
    await user.click(await screen.findByRole("option", { name: "Review uncommitted changes" }));

    expect(mocks.startThread).toHaveBeenCalledWith("/projects/example", null, null, null);
    expect(mocks.startReview).toHaveBeenCalledWith("thread-2", { type: "uncommittedChanges" });
  });

  it("runs a typed instruction as a custom review without asking", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "/review the auth changes{Enter}");

    expect(screen.queryByRole("option", { name: "Review uncommitted changes" })).not.toBeInTheDocument();
    expect(mocks.startReview).toHaveBeenCalledWith("thread-1", {
      type: "custom",
      instructions: "the auth changes",
    });
  });

  // A review sends no `turn/started`, so before this the turn only existed once
  // its first item streamed — and Stop in that window named no turn at all.
  it("can be stopped before any review output arrives", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "/review{Enter}");
    await user.click(await screen.findByRole("option", { name: "Review uncommitted changes" }));
    await user.click(await screen.findByRole("button", { name: "Stop" }));

    expect(mocks.interruptTurn).toHaveBeenCalledWith("thread-1", "review-turn-1");
  });

  it("says why it did nothing when a turn is already running", async () => {
    const user = userEvent.setup();
    mocks.readThread.mockResolvedValueOnce(
      detail({
        id: "turn-1",
        status: "inProgress",
        items: [{ id: "user-1", type: "userMessage", content: [{ type: "text", text: "Do the work" }] }],
      }),
    );
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example" });
    // A live turn is what makes the composer offer Stop instead of Send.
    await screen.findByRole("button", { name: "Stop" });

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "/review{Enter}");

    expect(
      await screen.findByText("/review can't start while Codex is working — stop the current turn first."),
    ).toBeVisible();
    expect(mocks.startReview).not.toHaveBeenCalled();
  });
});

describe("ThreadView /goal", () => {
  const composerLabel = "Message Codex… (@ to attach files, / for commands)";

  beforeEach(() => {
    resetSessions();
    mocks.readThread.mockReset();
    mocks.startThread.mockReset();
    mocks.startThread.mockResolvedValue({ id: "thread-2", cwd: "/projects/example" });
    mocks.startTurn.mockReset();
    mocks.requestAutoName.mockReset();
    mocks.setThreadGoal.mockReset();
    mocks.getThreadGoal.mockReset();
    mocks.clearThreadGoal.mockReset();
    mocks.setThreadGoal.mockImplementation((threadId: string, objective: string) =>
      Promise.resolve({ threadId, objective, status: "active", tokenBudget: null, tokensUsed: 0, timeUsedSeconds: 0 }),
    );
    mocks.getThreadGoal.mockResolvedValue(null);
    mocks.clearThreadGoal.mockResolvedValue(undefined);
    mocks.gitRepoInfo.mockResolvedValue({ isGitRepo: true, root: "/projects/example" });
    mocks.gitRecentCommits.mockResolvedValue([]);
  });

  function renderDraft() {
    render(ThreadView, { threadId: null, cwd: "/projects/example", projectPath: "/projects/example" });
    return screen.getByRole("textbox", { name: composerLabel });
  }

  it("starts a thread for the goal rather than dropping the command", async () => {
    const user = userEvent.setup();
    await user.type(renderDraft(), "/goal ship the auth refactor{Enter}");

    expect(mocks.startThread).toHaveBeenCalledWith("/projects/example", null, null, null);
    expect(mocks.setThreadGoal).toHaveBeenCalledWith("thread-2", "ship the auth refactor");
    expect(await screen.findByText("Goal set: ship the auth refactor")).toBeVisible();
    // No turn: the goal is the whole command, the composer stays free for the
    // opening message.
    expect(mocks.startTurn).not.toHaveBeenCalled();
    // The goal stays visible after the notice is gone.
    const banner = screen.getByTestId("goal-banner");
    expect(banner).toHaveTextContent("Goal · active");
    expect(banner).toHaveTextContent("ship the auth refactor");
    expect(screen.getByRole("button", { name: "Pause goal" })).toBeVisible();
  });

  it("shows the goal an open thread already carries, and follows updates", async () => {
    mocks.getThreadGoal.mockResolvedValue({
      threadId: "thread-1",
      objective: "keep the build green",
      status: "paused",
      tokenBudget: null,
      tokensUsed: 0,
      timeUsedSeconds: 0,
    });
    render(ThreadView, { threadId: "thread-1", cwd: "/projects/example", projectPath: "/projects/example" });
    expect(await screen.findByTestId("goal-banner")).toHaveTextContent("Goal · paused");
    expect(screen.getByRole("button", { name: "Resume goal" })).toBeVisible();

    for (const handler of [...mocks.handlers]) {
      handler({
        method: "thread/goal/updated",
        params: {
          threadId: "thread-1",
          goal: {
            threadId: "thread-1",
            objective: "keep the build green",
            status: "complete",
            tokenBudget: null,
            tokensUsed: 5,
            timeUsedSeconds: 1,
          },
        },
      });
    }
    await waitFor(() => expect(screen.getByTestId("goal-banner")).toHaveTextContent("Goal · complete"));
  });

  it("names the new thread from the objective, since it has no turn to name it from", async () => {
    const user = userEvent.setup();
    await user.type(renderDraft(), "/goal ship the auth refactor{Enter}");

    await screen.findByText("Goal set: ship the auth refactor");
    expect(mocks.requestAutoName).toHaveBeenCalledWith("thread-2", "seed", "ship the auth refactor");
  });

  it("answers a bare /goal on a draft without creating a thread", async () => {
    const user = userEvent.setup();
    await user.type(renderDraft(), "/goal{Enter}");

    expect(await screen.findByText("No goal is set — /goal <objective> sets one.")).toBeVisible();
    expect(mocks.startThread).not.toHaveBeenCalled();
    expect(mocks.getThreadGoal).not.toHaveBeenCalled();
  });

  it("does not create a thread just to clear a goal it cannot have", async () => {
    const user = userEvent.setup();
    await user.type(renderDraft(), "/goal clear{Enter}");

    expect(await screen.findByText("No goal is set — /goal <objective> sets one.")).toBeVisible();
    expect(mocks.startThread).not.toHaveBeenCalled();
    expect(mocks.clearThreadGoal).not.toHaveBeenCalled();
  });

  it("gives the objective back to the composer when setting it fails", async () => {
    const user = userEvent.setup();
    mocks.setThreadGoal.mockRejectedValueOnce(new Error("goals are unavailable"));
    const editor = renderDraft();

    await user.type(editor, "/goal ship the auth refactor{Enter}");

    await vi.waitFor(() => expect(mocks.toastError).toHaveBeenCalledWith("goals are unavailable"));
    expect(editor).toHaveTextContent("/goal ship the auth refactor");
  });

  it("sets the goal on an established thread without starting another", async () => {
    const user = userEvent.setup();
    await renderTurn(completedTurn());

    await user.type(screen.getByRole("textbox", { name: composerLabel }), "/goal ship the auth refactor{Enter}");

    expect(mocks.setThreadGoal).toHaveBeenCalledWith("thread-1", "ship the auth refactor");
    expect(mocks.startThread).not.toHaveBeenCalled();
    expect(mocks.requestAutoName).not.toHaveBeenCalled();
  });
});
