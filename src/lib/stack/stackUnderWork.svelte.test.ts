import { flushSync } from "svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SCENARIO_DIR, seedInboxScenario } from "$lib/inbox/inboxScenario";
import { inboxes } from "$lib/inbox/inboxStore.svelte";
import { resetPrLinks } from "$lib/review/prLinks.svelte";
import { previewCommands } from "$lib/services/preview/commands";
import type { StackSeed } from "$lib/services/preview/desk";
import { followUps, resetFollowUps } from "$lib/stack/followUps.svelte";
import { followUpKey } from "$lib/stack/stack";
import { type StackQuestion, StackUnderWork } from "$lib/stack/stackUnderWork.svelte";

// The scenario's stack 7, bottom first: #412, #415, #418, three behind main,
// in the worktree `harness-stack`; a restack stops at a conflict in #415.
const MAIN = SCENARIO_DIR;
const WORKTREE = "/Users/ciaran/worktrees/codex-custom/harness-stack";

const spy = <K extends keyof typeof previewCommands>(name: K) => vi.spyOn(previewCommands, name);
const order = (mock: { mock: { invocationCallOrder: number[] } }) => mock.mock.invocationCallOrder;

let stops: (() => void)[] = [];

/** Open stack 7 as the Stack page would, answering every question with
 *  `answer`, and wait until what is on disk is read. */
async function open(answer: boolean = true, seed: Partial<StackSeed> = {}) {
  seedInboxScenario(seed);
  const asked: StackQuestion[] = [];
  let stack!: StackUnderWork;
  stops.push(
    $effect.root(() => {
      stack = new StackUnderWork({
        repoDir: MAIN,
        repoName: "codex-custom",
        number: 7,
        confirm: async (question) => {
          asked.push(question);
          return answer;
        },
      });
    }),
  );
  flushSync();
  await stack.load();
  await vi.waitFor(() => expect(stack.overview).not.toBeNull());
  return { stack, asked };
}

/** Rewrite stack 7 in the inbox, as a sync that saw GitHub change would. */
function onGitHub(change: (prs: { number: number; merged: boolean; state: string }[]) => void) {
  const stack = inboxes[MAIN]?.stacks?.stacks.find((candidate) => candidate.number === 7);
  if (!stack) throw new Error("fixture stack missing");
  change(stack.prs);
}

beforeEach(() => {
  resetFollowUps();
  resetPrLinks();
  localStorage.clear();
  for (const key of Object.keys(inboxes)) delete inboxes[key];
});

afterEach(() => {
  for (const stop of stops) stop();
  stops = [];
  vi.restoreAllMocks();
});

describe("reading the stack", () => {
  it("reads the inbox, then what is on disk, and the restack preview while the base is ahead", async () => {
    const { stack } = await open();

    expect(stack.openPrs.map((pr) => pr.number)).toEqual([412, 415, 418]);
    expect(stack.overview?.worktree?.path).toBe(WORKTREE);
    expect(stack.theirs).toBe(false);
    await vi.waitFor(() => expect(stack.preview?.steps.map((step) => step.number)).toEqual([412, 415, 418]));
    await vi.waitFor(() => expect(stack.checkouts?.get(415)?.path).toBe(WORKTREE));
  });

  it("reads the preview for a PR that left the head below it, though the base has not moved", async () => {
    const { stack } = await open(true, { behind: 0, drifted: [418] });

    expect(stack.overview?.baseAhead).toBe(0);
    await vi.waitFor(() => expect(stack.behind.get(418)).toBe(1));
    await vi.waitFor(() => expect(stack.preview?.rewritesPushed).toEqual([412, 415, 418]));
  });

  it("reads no preview for a stack level with its base", async () => {
    const plan = spy("reviewRestackPlan");
    const { stack } = await open(true, { behind: 0, drifted: [] });
    await vi.waitFor(() => expect(stack.behind.size).toBe(3));

    expect(stack.preview).toBeNull();
    expect(plan).not.toHaveBeenCalled();
  });

  it("is someone else's when the viewer wrote none of its open PRs", async () => {
    const { stack } = await open();
    const inbox = inboxes[MAIN]?.inbox;
    if (inbox) inbox.viewer = "jo-r";

    expect(stack.theirs).toBe(true);
  });
});

describe("restacking", () => {
  it("asks with the plan, stops at the conflict, and abort puts every git branch back", async () => {
    const { stack, asked } = await open();

    expect(await stack.restack()).toBe(false);

    expect(asked).toHaveLength(1);
    const question = asked[0];
    if (question.kind !== "restack") throw new Error(`asked ${question.kind}`);
    expect(question).toMatchObject({ from: null, running: 0 });
    expect(question.plan.rewritesPushed).toEqual([412, 415, 418]);
    expect(question.plan.steps.find((step) => step.number === 415)?.conflicts).toEqual([
      "src-tauri/src/harness/driver.rs",
    ]);
    expect(stack.restacking).toBe(true);
    expect(stack.conflict?.number).toBe(415);
    expect(stack.progress?.branches.map((branch) => branch.status)).toEqual(["replayed", "conflict", "waiting"]);

    await stack.abortRestack();
    expect(stack.progress).toBeNull();
    expect(stack.restacking).toBe(false);
    expect(stack.busy).toBeNull();
  });

  it("runs nothing when the restack is declined", async () => {
    const run = spy("reviewRestack");
    const { stack, asked } = await open(false);

    expect(await stack.restack(415)).toBe(false);
    expect(asked[0]).toMatchObject({ kind: "restack", from: 415 });
    expect(run).not.toHaveBeenCalled();
    expect(stack.progress).toBeNull();
  });

  it("asks nothing when the stack already sits on its base", async () => {
    const run = spy("reviewRestack");
    const { stack, asked } = await open(true, { behind: 0, drifted: [] });

    expect(await stack.restack()).toBe(true);
    expect(asked).toEqual([]);
    expect(run).not.toHaveBeenCalled();
  });

  it("continues a resolved restack to the top", async () => {
    const { stack } = await open();
    await stack.restack();

    await stack.continueRestack();

    expect(stack.progress?.done).toBe(true);
    expect(stack.restacking).toBe(false);
    expect(stack.conflict).toBeNull();
  });

  it("starts one once loaded when opened for a restack, unless one is under way", async () => {
    seedInboxScenario();
    const run = spy("reviewRestack");
    let stack!: StackUnderWork;
    stops.push(
      $effect.root(() => {
        stack = new StackUnderWork({ repoDir: MAIN, repoName: "codex-custom", number: 7, confirm: async () => true });
      }),
    );
    flushSync();

    await stack.load({ restack: true });
    expect(run).toHaveBeenCalledOnce();
    expect(stack.conflict?.number).toBe(415);

    await stack.load({ restack: true });
    expect(run).toHaveBeenCalledOnce();
  });

  it("starts the thread that resolves a conflict in the worktree, linked to the conflicted PR before its turn", async () => {
    const started = spy("startThread");
    const link = spy("reviewLinkThread");
    const turn = spy("startTurn");
    const { stack } = await open();
    await stack.restack();

    const thread = await stack.resolveInThread();

    expect(thread?.cwd).toBe(WORKTREE);
    expect(started).toHaveBeenCalledWith(WORKTREE, null, null);
    expect(link).toHaveBeenCalledWith(MAIN, thread?.threadId, expect.objectContaining({ number: 415 }));
    expect(order(started)[0]).toBeLessThan(order(link)[0]);
    expect(order(link)[0]).toBeLessThan(order(turn)[0]);
  });
});

describe("pushing", () => {
  it("after a restack finishes, pushes from the worktree without asking, and the result card goes", async () => {
    const push = spy("reviewPushStack");
    const { stack, asked } = await open();
    await stack.restack();
    await stack.continueRestack();
    expect(stack.progress?.done).toBe(true);

    expect(await stack.push()).toBe(true);

    expect(asked.map((question) => question.kind)).toEqual(["restack"]);
    expect(push).toHaveBeenCalledWith(MAIN, expect.objectContaining({ number: 7 }), null);
    expect(stack.progress).toBeNull();
  });

  it("asks before pushing to someone else's git branch from the worktree, and pushes nothing when declined", async () => {
    const push = spy("reviewPushStack");
    const { stack, asked } = await open(false);
    const inbox = inboxes[MAIN]?.inbox;
    if (inbox) inbox.viewer = "jo-r";

    expect(await stack.push(415)).toBe(false);

    expect(asked).toEqual([
      {
        kind: "pushToTheirBranch",
        target: expect.objectContaining({ number: 415, headRef: "feat/claude-driver", mine: false }),
        worktreePath: WORKTREE,
      },
    ]);
    expect(push).not.toHaveBeenCalled();
  });
});

describe("editing the stack", () => {
  it("reorders after asking, then shows the new order from GitHub", async () => {
    const reorder = spy("reviewReorderStack");
    const { stack, asked } = await open();

    expect(await stack.edit({ kind: "reorder", order: [412, 418, 415] })).toBe(true);

    expect(asked).toEqual([{ kind: "reorder", order: [412, 418, 415] }]);
    expect(reorder).toHaveBeenCalledWith(MAIN, expect.objectContaining({ number: 7 }), [412, 418, 415]);
    expect(stack.openPrs.map((pr) => pr.number)).toEqual([412, 418, 415]);
    expect(stack.busy).toBeNull();
  });

  it("folds into the PR below after asking, and the folded PR leaves the stack", async () => {
    const { stack, asked } = await open();

    expect(await stack.edit({ kind: "fold", number: 415 })).toBe(true);

    expect(asked).toEqual([
      { kind: "fold", pr: expect.objectContaining({ number: 415 }), parent: expect.objectContaining({ number: 412 }) },
    ]);
    expect(stack.openPrs.map((pr) => pr.number)).toEqual([412, 418]);
  });

  it("changes nothing when a remove is declined, and cannot fold the bottom PR", async () => {
    const remove = spy("reviewRemoveFromStack");
    const fold = spy("reviewFoldIntoParent");
    const { stack, asked } = await open(false);

    expect(await stack.edit({ kind: "remove", number: 418 })).toBe(false);
    expect(await stack.edit({ kind: "fold", number: 412 })).toBe(false);

    expect(asked.map((question) => question.kind)).toEqual(["remove"]);
    expect(remove).not.toHaveBeenCalled();
    expect(fold).not.toHaveBeenCalled();
  });

  it("renames without asking, and the stack follows the new name", async () => {
    const { stack, asked } = await open();

    expect(await stack.edit({ kind: "rename", number: 418, to: "feat/maki" })).toBe(true);

    expect(asked).toEqual([]);
    expect(stack.openPrs.at(-1)?.headRef).toBe("feat/maki");
  });

  it("is busy while an edit runs, and re-reads the stack after one fails", async () => {
    let busy: unknown = "not run";
    spy("reviewAddStackBranch").mockImplementation(async () => {
      busy = stack.busy;
      throw new Error("feat/new already exists");
    });
    const overview = spy("reviewStackOverview");
    const { stack } = await open();
    overview.mockClear();

    expect(await stack.edit({ kind: "add", name: "feat/new", below: null })).toBe(false);

    expect(busy).toBe("add");
    expect(stack.busy).toBeNull();
    expect(overview).toHaveBeenCalled();
  });
});

describe("after a PR merges", () => {
  it("offers to restack the rest as soon as the inbox sees the merge, until put off", async () => {
    const { stack } = await open();
    expect(stack.followUp).toBeNull();

    onGitHub((prs) => {
      prs[0] = { ...prs[0], merged: true, state: "closed" };
    });

    await vi.waitFor(() => expect(stack.followUp).toMatchObject({ kind: "restack", merged: [412], rest: [415, 418] }));
    stack.putOffFollowUp();
    expect(stack.followUp).toBeNull();
  });

  it("restacks, then deletes the merged git branches; deletes nothing when the restack is declined", async () => {
    const remove = spy("reviewDeleteMergedBranches");
    const declined = await open(false, { merged: [412], conflict: null });
    expect(await declined.stack.cleanUpMerged()).toBe(false);
    expect(remove).not.toHaveBeenCalled();

    const { stack } = await open(true, { merged: [412], conflict: null });
    expect(await stack.cleanUpMerged()).toBe(true);
    expect(stack.progress?.done).toBe(true);
    expect(remove).toHaveBeenCalledOnce();
  });

  it("once every PR merged, asks, then removes the worktree", async () => {
    const remove = spy("gitWorktreeRemove");
    const { stack, asked } = await open(true, { merged: [412, 415, 418] });
    expect(stack.followUp).toEqual({ kind: "cleanup" });

    expect(await stack.removeWorktree()).toBe(true);

    expect(asked).toEqual([{ kind: "removeWorktree", path: WORKTREE, threads: 0 }]);
    expect(remove).toHaveBeenCalledWith(MAIN, WORKTREE, false);
  });

  it("settles the sidebar's dot when a merged PR leaves nothing to offer", async () => {
    const read = previewCommands.reviewStackOverview;
    spy("reviewStackOverview").mockImplementation(async (repoDir, input) => ({
      ...(await read(repoDir, input)),
      mergedBranches: [],
    }));
    const { stack } = await open(true, { merged: [412] });
    const input = stack.input;
    if (!input) throw new Error("fixture stack missing");

    await vi.waitFor(() => expect(followUps.later).toContain(followUpKey(input)));
    expect(stack.followUp).toBeNull();
  });
});

describe("reviewing it", () => {
  it("queues one agent review over the open PRs, bottom first", async () => {
    const enqueue = spy("reviewEnqueueAgentReview");
    spy("reviewClaimAgentReviews").mockResolvedValue([]);
    const { stack } = await open();

    await stack.reviewWhole();

    expect(enqueue).toHaveBeenCalledOnce();
    const request = enqueue.mock.calls[0][1];
    expect(request.targets.map((target) => target.number)).toEqual([412, 415, 418]);
  });
});
