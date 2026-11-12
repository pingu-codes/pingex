import { flushSync } from "svelte";
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from "vitest";
import { appData, trackNewThread } from "$lib/app/appData.svelte";
import { type PreviewCommands, previewCommands } from "$lib/services/preview/commands";
import { seedPreviewDesk } from "$lib/services/preview/desk";
import { deskRepository, prCheckout } from "$lib/services/preview/factories";
import { type DraftDeps, DraftThread, startThreadWith } from "$lib/thread/draftThread.svelte";
import { resetSessions } from "$lib/thread/sessions.svelte";
import type { ThreadSession } from "$lib/thread/threadSession.svelte";
import type { BootstrapData, UserInputPart } from "$lib/types";

// The preview's command fakes answer everything here — repositories, worktrees,
// PR worktree context, thread creation. Spies watch the calls; a turn is only
// started, never streamed, so no fake events outlive a test.

const PR_WORKTREE = "/wt/pr-7";
const said = (text: string): UserInputPart[] => [{ type: "text", text }];
const fakes = previewCommands as Required<PreviewCommands>;
type Spy<K extends keyof PreviewCommands> = MockInstance<NonNullable<PreviewCommands[K]>>;

let startThread: Spy<"startThread">;
let startTurn: Spy<"startTurn">;
let autoName: Spy<"autoNameThread">;
let worktreeAddTemp: Spy<"gitWorktreeAddTemp">;
let worktreeAdd: Spy<"gitWorktreeAdd">;
let cleanup: (() => void) | null = null;
let savedData: BootstrapData | null = null;

beforeEach(() => {
  resetSessions();
  startThread = vi.spyOn(fakes, "startThread");
  startTurn = vi.spyOn(fakes, "startTurn").mockImplementation(() => ({
    id: "turn-1",
    status: "inProgress",
    items: [],
  }));
  autoName = vi.spyOn(fakes, "autoNameThread");
  worktreeAddTemp = vi.spyOn(fakes, "gitWorktreeAddTemp");
  worktreeAdd = vi.spyOn(fakes, "gitWorktreeAdd");
  savedData = appData.data;
});

afterEach(() => {
  cleanup?.();
  cleanup = null;
  vi.restoreAllMocks();
  appData.data = savedData;
  resetSessions();
});

/** A draft as a view holds it, with the session the view would show. */
function draft({ cwd = "/projects/example", ...overrides }: Partial<Omit<DraftDeps, "cwd">> & { cwd?: string } = {}) {
  const onCreated = vi.fn<DraftDeps["onCreated"]>();
  const view = { session: null as unknown as ThreadSession };
  let made!: DraftThread;
  cleanup = $effect.root(() => {
    made = new DraftThread({
      threadId: () => null,
      cwd: () => cwd,
      projectPath: () => cwd,
      workspaceId: () => null,
      session: () => view.session,
      harness: () => null,
      includeWorktreeContext: () => true,
      onCreated,
      ...overrides,
    });
  });
  flushSync();
  view.session = made.session();
  return { draft: made, session: view.session, onCreated };
}

/** The draft has read its repository and Worktree context. */
async function settled(made: DraftThread) {
  await vi.waitFor(() => expect(made.repoInfo).not.toBeNull());
}

function seedPrWorktree() {
  seedPreviewDesk({
    repositories: [
      deskRepository("pingex/app", {
        repository: { path: "/projects/app" },
        checkouts: [prCheckout(7, { path: PR_WORKTREE, managed: true, step: "worktree" })],
      }),
    ],
  });
}

describe("where a draft starts", () => {
  it("starts in the project's own directory, on the chosen harness", async () => {
    const { draft: made, session, onCreated } = draft({ harness: () => "claude" });

    expect(await made.send(session, said("Coordinate the release"))).toBe(true);

    expect(startThread).toHaveBeenCalledWith("/projects/example", null, "claude");
    const id = session.id;
    expect(id).toMatch(/^preview-/);
    expect(onCreated).toHaveBeenCalledWith(id, "/projects/example", false);
    expect(startTurn).toHaveBeenCalledWith(id, said("Coordinate the release"), null);
  });

  it("starts a workspace's thread on its hub with the workspace id", async () => {
    const { draft: made, session } = draft({ cwd: "/workspace/hub", workspaceId: () => "workspace-1" });

    await made.send(session, said("Coordinate the release"));

    expect(startThread).toHaveBeenCalledWith("/workspace/hub", "workspace-1", null);
    expect(startTurn).toHaveBeenCalledWith(session.id, said("Coordinate the release"), null);
  });

  it("makes a temporary worktree only once the thread starts", async () => {
    const { draft: made, session, onCreated } = draft();
    await settled(made);

    made.startIn("temporary");
    expect(worktreeAddTemp).not.toHaveBeenCalled();
    await made.send(session, said("Try it on the side"));

    expect(worktreeAddTemp).toHaveBeenCalledWith("/projects/example");
    const path = made.worktreePath;
    expect(path).toContain("/worktrees-tmp/example/");
    expect(startThread).toHaveBeenCalledWith(path, null, null);
    expect(onCreated).toHaveBeenCalledWith(session.id, path, true);
  });

  it("starts in a permanent worktree made when it is chosen", async () => {
    const { draft: made, session } = draft();
    await settled(made);
    const branch = { kind: "new", name: "feature/x", base: null } as const;

    await made.addWorktree("/codex/worktrees/x", branch);
    expect(worktreeAdd).toHaveBeenCalledWith("/projects/example", { path: "/codex/worktrees/x", branch });
    expect(made.location).toBe("permanent");
    await made.send(session, said("Build it there"));

    expect(startThread).toHaveBeenCalledWith("/codex/worktrees/x", null, null);
  });

  it("stays a draft and says why when the thread cannot be created", async () => {
    startThread.mockImplementation(() => {
      throw new Error("Codex refused the thread");
    });
    const { draft: made, session, onCreated } = draft();

    expect(await made.send(session, said("Do the work"))).toBe(false);

    expect(session.id).toBeNull();
    expect(session.starting).toBe(false);
    expect(session.streamError).toBe("Codex refused the thread");
    expect(session.thread?.turns).toEqual([]);
    expect(startTurn).not.toHaveBeenCalled();
    expect(autoName).not.toHaveBeenCalled();
    expect(onCreated).not.toHaveBeenCalled();
  });

  it("sends later messages on the thread it became", async () => {
    // A first turn that is already over, so the next message is not queued.
    startTurn.mockImplementationOnce(() => ({ id: "turn-1", status: "completed", items: [] }));
    const { draft: made, session } = draft();
    await made.send(session, said("First"));
    startTurn.mockClear();

    await made.send(session, said("Second"));

    expect(startThread).toHaveBeenCalledTimes(1);
    expect(startTurn).toHaveBeenCalledWith(session.id, said("Second"), null);
  });
});

describe("Worktree context", () => {
  beforeEach(seedPrWorktree);

  it("tells the agent about a PR worktree after the user's words, and names from the words alone", async () => {
    const { draft: made, session } = draft({ cwd: PR_WORKTREE });
    await vi.waitFor(() => expect(made.worktreeContext?.purpose.kind).toBe("managedPr"));
    const context = made.worktreeContext?.text ?? "";

    await made.send(session, said("Fix the tax rounding"));

    expect(startTurn.mock.calls[0][1]).toEqual([
      { type: "text", text: "Fix the tax rounding" },
      { type: "text", text: `\n\n<worktree_context>\n${context}\n</worktree_context>` },
    ]);
    await vi.waitFor(() => expect(autoName).toHaveBeenCalledWith(session.id, "Fix the tax rounding"));
  });

  it("sends nothing extra when the composer leaves it out", async () => {
    const { draft: made, session } = draft({ cwd: PR_WORKTREE, includeWorktreeContext: () => false });
    await vi.waitFor(() => expect(made.worktreeContext).not.toBeNull());

    await made.send(session, said("Fix the tax rounding"));

    expect(startTurn.mock.calls[0][1]).toEqual(said("Fix the tax rounding"));
  });

  it("offers nothing in a directory that is not a PR or Stack worktree", async () => {
    const { draft: made, session } = draft({ cwd: "/projects/example" });
    await settled(made);

    expect(made.worktreeContext).toBeNull();
    await made.send(session, said("Fix the tax rounding"));
    expect(startTurn.mock.calls[0][1]).toEqual(said("Fix the tax rounding"));
  });

  it("withdraws the offer when the thread will start in a temporary worktree or a workspace", async () => {
    const { draft: made } = draft({ cwd: PR_WORKTREE });
    await vi.waitFor(() => expect(made.worktreeContext).not.toBeNull());

    made.startIn("temporary");
    expect(made.worktreeContext).toBeNull();
    made.startIn("project");
    expect(made.worktreeContext).not.toBeNull();

    const { draft: hub } = draft({ cwd: PR_WORKTREE, workspaceId: () => "workspace-1" });
    await settled(hub);
    expect(hub.worktreeContext).toBeNull();
  });
});

describe("naming a new thread", () => {
  function showProject() {
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
      threadBranches: [],
      subagents: [],
      unfiledThreads: [],
      sections: [],
      sectionsSupported: false,
      sidebarLayout: { folders: [], placements: [] },
    } as unknown as BootstrapData;
  }

  it("titles the sidebar entry and seeds the namer without waiting for the turn to start", async () => {
    showProject();
    // A turn that never starts stands in for the seconds one normally takes.
    startTurn.mockImplementation(() => new Promise(() => {}) as never);
    const { draft: made, session } = draft({
      // What the app shell does with a draft that has become a real thread.
      onCreated: (id, cwd) => trackNewThread(id, cwd),
    });

    void made.send(session, said("Refactor the sidebar grouping\nand more"));

    await vi.waitFor(() => expect(startTurn).toHaveBeenCalled());
    expect(appData.data?.projects[0].threads[0]?.title).toBe("Refactor the sidebar grouping");
    await vi.waitFor(() =>
      expect(autoName).toHaveBeenCalledWith(session.id, "Refactor the sidebar grouping\nand more"),
    );
    // The optimistic bubble is up while the turn starts.
    expect(session.thread?.turns.at(-1)?.items[0].content).toEqual(said("Refactor the sidebar grouping\nand more"));
  });

  it("seeds no namer for an opening message with no words", async () => {
    const { draft: made, session } = draft();

    await made.send(session, [{ type: "image", url: "data:image/png;base64,AAA" }]);

    expect(startTurn).toHaveBeenCalled();
    expect(autoName).not.toHaveBeenCalled();
  });

  it("titles a thread started outright once it is announced, after its turn starts", async () => {
    showProject();
    const order: string[] = [];
    startTurn.mockImplementation(() => {
      order.push("turn");
      return { id: "turn-1", status: "inProgress", items: [] };
    });

    const thread = await startThreadWith({ cwd: "/projects/example", harness: "claude" }, said("Implement the plan"), {
      options: { model: "gpt-5.2-codex" },
      beforeTurn: () => order.push("before"),
      afterTurn: (started) => {
        order.push("after");
        trackNewThread(started.id, "/projects/example");
      },
    });

    expect(order).toEqual(["before", "turn", "after"]);
    expect(startThread).toHaveBeenCalledWith("/projects/example", null, "claude");
    expect(startTurn).toHaveBeenCalledWith(thread.id, said("Implement the plan"), { model: "gpt-5.2-codex" });
    expect(appData.data?.projects[0].threads[0]?.title).toBe("Implement the plan");
    await vi.waitFor(() => expect(autoName).toHaveBeenCalledWith(thread.id, "Implement the plan"));
  });
});
