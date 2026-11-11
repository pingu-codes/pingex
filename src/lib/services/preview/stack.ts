// The Stack page's commands for the browser preview and the Stack page
// tests: each stack's lead over its base, its worktree, and a restack that
// stops once at the conflict its seed names before it can continue. Reorder,
// fold and remove edit the desk's live stack listing, so a re-synced inbox
// shows the new shape. What each stack does is in its `StackSeed`.

import type { PrBaseInput, RestackPlan, RestackProgress, StackInput, StackOverview, StackWorktree } from "$lib/types";
import type { DeskCommands } from "./commands";
import { liveRepo, type StackSim } from "./desk";

/** Stack `number`'s fake state; a stack the seed said nothing about is level
 *  with its base, with no worktree. */
function sim(repoDir: string, number: number): StackSim {
  const sims = liveRepo(repoDir).sims;
  let found = sims.get(number);
  if (!found) {
    found = { behind: 0, drifted: [], worktree: null, conflict: null, commits: new Map(), baseCommits: [], run: null };
    sims.set(number, found);
  }
  return found;
}

const open = (input: StackInput) => input.prs.filter((pr) => pr.open && !pr.merged);
const commitsOf = (entry: StackSim, number: number) => entry.commits.get(number) ?? 1;

/** Rewrite the live stack to `order` (bottom first), and each PR's base. */
function reshape(repoDir: string, input: StackInput, order: number[], closed: number[] = []) {
  const repo = liveRepo(repoDir);
  const stack = repo.stacks.stacks.find((candidate) => candidate.number === input.number);
  if (!stack) return;
  const byNumber = new Map(stack.prs.map((pr) => [pr.number, pr]));
  const kept = stack.prs.filter((pr) => pr.merged || pr.state !== "open");
  stack.prs = [...kept, ...order.flatMap((number) => byNumber.get(number) ?? [])];
  for (const number of closed) {
    const entry = byNumber.get(number);
    if (entry) stack.prs.push({ ...entry, state: "closed" });
  }
  const prs = repo.inbox.prs;
  order.forEach((number, index) => {
    const pr = prs.find((candidate) => candidate.number === number);
    const below = index > 0 ? byNumber.get(order[index - 1]) : null;
    if (pr) pr.baseRef = below?.headRef ?? input.baseRef;
  });
  repo.inbox.prs = prs.filter((pr) => !closed.includes(pr.number));
}

function overview(repoDir: string, input: StackInput): StackOverview {
  const entry = sim(repoDir, input.number);
  return {
    number: input.number,
    baseAhead: entry.behind,
    baseCommits: entry.baseCommits.slice(0, entry.behind),
    branches: open(input).map((pr) => ({
      number: pr.number,
      headRef: pr.headRef,
      tip: pr.headSha,
      commits: commitsOf(entry, pr.number),
      unpushed: false,
      local: entry.worktree !== null,
      // Newest first, as `git log` gives them.
      commitLines: Array.from({ length: commitsOf(entry, pr.number) }, (_, commit) => ({
        sha: `${pr.number}${commit}`.padEnd(40, "a"),
        subject: `${pr.title || pr.headRef}: part ${commit + 1}`,
      })).reverse(),
    })),
    worktree: entry.worktree ? { ...entry.worktree } : null,
    mergedBranches: input.prs.filter((pr) => pr.merged).map((pr) => pr.number),
    restack: entry.run ? structuredClone(entry.run) : null,
  };
}

function plan(repoDir: string, input: StackInput, from: number | null): RestackPlan {
  const entry = sim(repoDir, input.number);
  const prs = open(input);
  const start =
    from === null
      ? 0
      : Math.max(
          0,
          prs.findIndex((pr) => pr.number === from),
        );
  const moving = entry.behind > 0 || entry.drifted.length > 0 || from !== null;
  const steps = prs.slice(start).map((pr, offset) => {
    const index = start + offset;
    return {
      number: pr.number,
      headRef: pr.headRef,
      onto: index === 0 ? input.baseRef : prs[index - 1].headRef,
      commits: commitsOf(entry, pr.number),
      unchanged: !moving,
      conflicts: moving && entry.conflict?.at === pr.number ? [...entry.conflict.files] : [],
    };
  });
  return {
    ontoRef: input.baseRef,
    steps,
    rewritesPushed: steps.filter((step) => !step.unchanged).map((step) => step.number),
    predicted: true,
    worktree: entry.worktree?.path ?? null,
  };
}

/** The open stack PR `number` is an open member of. */
function stackOf(repoDir: string, number: number) {
  return liveRepo(repoDir).stacks.stacks.find(
    (candidate) =>
      candidate.open &&
      candidate.prs.some((entry) => entry.number === number && entry.state === "open" && !entry.merged),
  );
}

/** A bottom PR is as far behind as its stack; one above is 1 behind the PR
 *  below when it has drifted. */
function prBehind(repoDir: string, pr: PrBaseInput): number {
  const stack = stackOf(repoDir, pr.number);
  if (!stack) return liveRepo(repoDir).behind.get(pr.number) ?? 0;
  const entry = sim(repoDir, stack.number);
  if (pr.baseRef === stack.baseRef) return entry.behind;
  return entry.drifted.includes(pr.number) ? 1 : 0;
}

/** The worktree of the stack PR `number` is in, if it has one. */
export function stackWorktree(repoDir: string, number: number): StackWorktree | null {
  const stack = stackOf(repoDir, number);
  return stack ? sim(repoDir, stack.number).worktree : null;
}

/** Put the stack PR `number` is in at `worktree`. */
export function setStackWorktree(repoDir: string, number: number, worktree: StackWorktree | null): void {
  const stack = stackOf(repoDir, number);
  if (stack) sim(repoDir, stack.number).worktree = worktree;
}

function finish(entry: StackSim, run: RestackProgress): RestackProgress {
  for (const branch of run.branches) {
    if (branch.status === "waiting" || branch.status === "conflict") {
      branch.status = "replayed";
      branch.conflicts = [];
    }
  }
  run.done = true;
  entry.run = null;
  entry.behind = 0;
  entry.drifted = [];
  entry.conflict = null;
  return run;
}

export const stackCommands = {
  reviewStackOverview: (repoDir, input) => overview(repoDir, input),
  reviewPrsBehind: (repoDir, prs) => prs.map((pr) => ({ number: pr.number, behind: prBehind(repoDir, pr) })),
  reviewRestackPlan: (repoDir, input, from) => plan(repoDir, input, from),
  reviewRestack(repoDir, input, from) {
    const entry = sim(repoDir, input.number);
    if (!entry.worktree) throw new Error("The stack needs a worktree first: check one of its git branches out");
    const steps = plan(repoDir, input, from).steps;
    const conflictAt = steps.findIndex((step) => step.conflicts.length > 0);
    const run: RestackProgress = {
      stack: input.number,
      worktree: entry.worktree.path,
      ontoRef: input.baseRef,
      branches: steps.map((step, index) => ({
        number: step.number,
        headRef: step.headRef,
        status:
          conflictAt < 0 || index < conflictAt
            ? step.unchanged
              ? "unchanged"
              : "replayed"
            : index === conflictAt
              ? "conflict"
              : "waiting",
        commits: step.commits,
        conflicts: index === conflictAt ? step.conflicts : [],
      })),
      done: false,
    };
    if (conflictAt < 0) return finish(entry, run);
    entry.run = run;
    return structuredClone(run);
  },
  reviewContinueRestack(repoDir, stack) {
    const entry = sim(repoDir, stack);
    if (!entry.run) throw new Error("There is no stopped restack to continue");
    return finish(entry, entry.run);
  },
  reviewAbortRestack(repoDir, stack) {
    sim(repoDir, stack).run = null;
    return null;
  },
  reviewPushStack: (_repoDir, input, only) => ({
    pushed: open(input)
      .filter((pr) => only === null || pr.number === only)
      .map((pr) => pr.headRef),
  }),
  reviewReorderStack(repoDir, input, order) {
    reshape(repoDir, input, order);
    return null;
  },
  reviewFoldIntoParent(repoDir, input, number) {
    reshape(
      repoDir,
      input,
      open(input)
        .map((pr) => pr.number)
        .filter((candidate) => candidate !== number),
      [number],
    );
    return null;
  },
  reviewRemoveFromStack(repoDir, input, number) {
    const repo = liveRepo(repoDir);
    reshape(
      repoDir,
      input,
      open(input)
        .map((pr) => pr.number)
        .filter((candidate) => candidate !== number),
    );
    const stack = repo.stacks.stacks.find((candidate) => candidate.number === input.number);
    if (stack) stack.prs = stack.prs.filter((pr) => pr.number !== number);
    const pr = repo.inbox.prs.find((candidate) => candidate.number === number);
    if (pr) pr.baseRef = input.baseRef;
    return null;
  },
  reviewSplitBranch: () => 430,
  reviewRenameStackBranch(repoDir, input, number, to) {
    const repo = liveRepo(repoDir);
    const stack = repo.stacks.stacks.find((candidate) => candidate.number === input.number);
    const entry = stack?.prs.find((pr) => pr.number === number);
    const pr = repo.inbox.prs.find((candidate) => candidate.number === number);
    if (entry) entry.headRef = to;
    if (pr) pr.headRef = to;
    return null;
  },
  reviewAddStackBranch: () => null,
  reviewDeleteMergedBranches: (_repoDir, input) => input.prs.filter((pr) => pr.merged).map((pr) => pr.headRef),
} satisfies Partial<DeskCommands>;
