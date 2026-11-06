// The Stack page's commands for the browser preview and the Stack page
// tests: each stack's lead over its base, its worktree and a restack that
// stops once at a conflict (in `driver.rs` on the second git branch) before
// it can continue. #418 has drifted off #415's head, and my single #128 is
// behind main. Reorder, fold and remove edit the desk's live stack
// listing, so a re-synced inbox shows the new shape. `resetPreviewStacks`
// puts everything back.

import type {
  CommitLine,
  InboxPr,
  PrBaseInput,
  RestackPlan,
  RestackProgress,
  StackInput,
  StackListing,
  StackOverview,
  StackWorktree,
} from "$lib/types";
import type { PreviewCommands } from "./commands";
import { deskLiveRepo } from "./desk";

type FakeStack = {
  behind: number;
  /** Open PRs no longer on the head of the PR below them. */
  drifted: number[];
  worktree: StackWorktree | null;
  /** The first restack stops at a conflict on the second git branch. */
  conflictAhead: boolean;
  run: RestackProgress | null;
};

const STACK_WORKTREE = "/Users/ciaran/worktrees/codex-custom/harness-stack";
const BASE_COMMITS: CommitLine[] = [
  { sha: "1a627a2".padEnd(40, "0"), subject: "fix: better thread placements" },
  { sha: "4ef2adb".padEnd(40, "0"), subject: "refactor: move to bun" },
  { sha: "b857f01".padEnd(40, "0"), subject: "feat: harness capabilities" },
];
/** Commits per git branch, by position from the bottom. */
const COMMITS = [3, 7, 4, 2, 2];

function initial(number: number): FakeStack {
  return number === 7
    ? {
        behind: 3,
        drifted: [418],
        worktree: { path: STACK_WORKTREE, branch: "feat/claude-driver", clean: true },
        conflictAhead: true,
        run: null,
      }
    : { behind: 1, drifted: [], worktree: null, conflictAhead: false, run: null };
}

/** PRs in no stack that are behind their base, by number. */
const SINGLES_BEHIND: Record<number, number> = { 128: 2 };

const stacks = new Map<string, FakeStack>();
let saved: Map<string, { prs: InboxPr[]; stacks: StackListing }> | null = null;

const key = (repoDir: string, number: number) => `${repoDir}#${number}`;

function fake(repoDir: string, number: number): FakeStack {
  const id = key(repoDir, number);
  let entry = stacks.get(id);
  if (!entry) {
    entry = initial(number);
    stacks.set(id, entry);
  }
  return entry;
}

const open = (input: StackInput) => input.prs.filter((pr) => pr.open && !pr.merged);

/** Remember the desk's listing before the first edit, for the reset. */
function live(repoDir: string) {
  saved ??= new Map();
  const repo = deskLiveRepo(repoDir);
  if (!saved.has(repoDir)) saved.set(repoDir, structuredClone(repo));
  return repo;
}

/** Rewrite the live stack to `order` (bottom first), and each PR's base. */
function reshape(repoDir: string, input: StackInput, order: number[], closed: number[] = []) {
  const repo = live(repoDir);
  const stack = repo.stacks.stacks.find((candidate) => candidate.number === input.number);
  if (!stack) return;
  const byNumber = new Map(stack.prs.map((pr) => [pr.number, pr]));
  const kept = stack.prs.filter((pr) => pr.merged || pr.state !== "open");
  stack.prs = [...kept, ...order.flatMap((number) => byNumber.get(number) ?? [])];
  for (const number of closed) {
    const entry = byNumber.get(number);
    if (entry) stack.prs.push({ ...entry, state: "closed" });
  }
  order.forEach((number, index) => {
    const pr = repo.prs.find((candidate) => candidate.number === number);
    const below = index > 0 ? byNumber.get(order[index - 1]) : null;
    if (pr) pr.baseRef = below?.headRef ?? input.baseRef;
  });
  repo.prs.splice(0, repo.prs.length, ...repo.prs.filter((pr) => !closed.includes(pr.number)));
}

function overview(repoDir: string, input: StackInput): StackOverview {
  const entry = fake(repoDir, input.number);
  return {
    number: input.number,
    baseAhead: entry.behind,
    baseCommits: BASE_COMMITS.slice(0, entry.behind),
    branches: open(input).map((pr, index) => ({
      number: pr.number,
      headRef: pr.headRef,
      tip: pr.headSha,
      commits: COMMITS[index] ?? 1,
      unpushed: false,
      // Newest first, as `git log` gives them.
      commitLines: Array.from({ length: COMMITS[index] ?? 1 }, (_, commit) => ({
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
  const entry = fake(repoDir, input.number);
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
      commits: COMMITS[index] ?? 1,
      unchanged: !moving,
      conflicts: moving && entry.conflictAhead && index === 1 ? ["src-tauri/src/harness/driver.rs"] : [],
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

/** A bottom PR is as far behind as its stack; one above is 1 behind the PR
 *  below when it has drifted. */
function prBehind(repoDir: string, pr: PrBaseInput): number {
  const stack = deskLiveRepo(repoDir).stacks.stacks.find(
    (candidate) =>
      candidate.open &&
      candidate.prs.some((entry) => entry.number === pr.number && entry.state === "open" && !entry.merged),
  );
  if (!stack) return SINGLES_BEHIND[pr.number] ?? 0;
  const entry = fake(repoDir, stack.number);
  if (pr.baseRef === stack.baseRef) return entry.behind;
  return entry.drifted.includes(pr.number) ? 1 : 0;
}

/** For tests: set how far stack `number` is behind, and which PRs drifted. */
export function setPreviewStackDrift(repoDir: string, number: number, behind: number, drifted: number[]) {
  const entry = fake(repoDir, number);
  entry.behind = behind;
  entry.drifted = [...drifted];
}

function finish(entry: FakeStack, run: RestackProgress): RestackProgress {
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
  entry.conflictAhead = false;
  return run;
}

export const previewStackCommands: PreviewCommands = {
  reviewStackOverview: (repoDir, input) => overview(repoDir, input),
  reviewPrsBehind: (repoDir, prs) => prs.map((pr) => ({ number: pr.number, behind: prBehind(repoDir, pr) })),
  reviewRestackPlan: (repoDir, input, from) => plan(repoDir, input, from),
  reviewRestack(repoDir, input, from) {
    const entry = fake(repoDir, input.number);
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
    const entry = fake(repoDir, stack);
    if (!entry.run) throw new Error("There is no stopped restack to continue");
    return finish(entry, entry.run);
  },
  reviewAbortRestack(repoDir, stack) {
    fake(repoDir, stack).run = null;
  },
  reviewPushStack: (_repoDir, input, only) => ({
    pushed: open(input)
      .filter((pr) => only === null || pr.number === only)
      .map((pr) => pr.headRef),
  }),
  reviewReorderStack(repoDir, input, order) {
    reshape(repoDir, input, order);
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
  },
  reviewRemoveFromStack(repoDir, input, number) {
    const repo = live(repoDir);
    reshape(
      repoDir,
      input,
      open(input)
        .map((pr) => pr.number)
        .filter((candidate) => candidate !== number),
    );
    const stack = repo.stacks.stacks.find((candidate) => candidate.number === input.number);
    if (stack) stack.prs = stack.prs.filter((pr) => pr.number !== number);
    const pr = repo.prs.find((candidate) => candidate.number === number);
    if (pr) pr.baseRef = input.baseRef;
  },
  reviewSplitBranch: () => 430,
  reviewRenameStackBranch(repoDir, input, number, to) {
    const repo = live(repoDir);
    const stack = repo.stacks.stacks.find((candidate) => candidate.number === input.number);
    const entry = stack?.prs.find((pr) => pr.number === number);
    const pr = repo.prs.find((candidate) => candidate.number === number);
    if (entry) entry.headRef = to;
    if (pr) pr.headRef = to;
  },
  reviewAddStackBranch: () => {},
  reviewDeleteMergedBranches: (_repoDir, input) => input.prs.filter((pr) => pr.merged).map((pr) => pr.headRef),
};

/** Back to the fixtures: no edits, every stack behind and conflicting once. */
export function resetPreviewStacks() {
  stacks.clear();
  if (!saved) return;
  for (const [repoDir, original] of saved) {
    const repo = deskLiveRepo(repoDir);
    repo.prs.splice(0, repo.prs.length, ...structuredClone(original.prs));
    repo.stacks.supported = original.stacks.supported;
    repo.stacks.stacks = structuredClone(original.stacks.stacks);
  }
  saved = null;
}
