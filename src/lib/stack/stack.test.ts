import { describe, expect, it } from "vitest";
import { deskInbox, deskStacks } from "$lib/services/preview/desk";
import {
  behindLabel,
  branchMenu,
  driftLabel,
  dropped,
  followUpWorktrees,
  mergeFollowUp,
  moved,
  myBaseInputs,
  stackInputFor,
} from "$lib/stack/stack";
import type { StackInput } from "$lib/types";

const MAIN = "/Users/ciaran/Projects/codex-custom";

function myStack(): StackInput {
  const stack = deskStacks(MAIN).stacks.find((candidate) => candidate.number === 7);
  if (!stack) throw new Error("fixture stack missing");
  return stackInputFor(stack, deskInbox(MAIN));
}

const ids = (entries: ReturnType<typeof branchMenu>) =>
  entries.flatMap((entry) => (entry.kind === "separator" ? [] : [entry.id]));

describe("the stack as the stack operations take it", () => {
  it("joins GitHub's listing with the inbox, bottom first, with the base's tip", () => {
    const input = myStack();
    expect(input.baseRef).toBe("main");
    expect(input.prs.map((pr) => [pr.number, pr.headRef, pr.open])).toEqual([
      [412, "feat/driver-trait", true],
      [415, "feat/claude-driver", true],
      [418, "feat/maki-acp", true],
    ]);
    expect(input.prs[1].title).toBe("Claude Code driver over stream-json");
    expect(input.baseSha).not.toBe("");
  });
});

describe("reordering", () => {
  it("moves a git branch one place, never past either end", () => {
    expect(moved([412, 415, 418], 415, "up")).toEqual([412, 418, 415]);
    expect(moved([412, 415, 418], 415, "down")).toEqual([415, 412, 418]);
    expect(moved([412, 415, 418], 418, "up")).toBeNull();
    expect(moved([412, 415, 418], 412, "down")).toBeNull();
  });

  it("drops a git branch onto the one it should build on", () => {
    expect(dropped([412, 415, 418], 418, 412)).toEqual([412, 418, 415]);
    expect(dropped([412, 415, 418], 418, null)).toEqual([418, 412, 415]);
    expect(dropped([412, 415, 418], 415, 412)).toBeNull();
    expect(dropped([412, 415, 418], 415, 415)).toBeNull();
  });
});

describe("the git branch menu", () => {
  it("offers every stack edit, folding into the PR below", () => {
    const menu = branchMenu(myStack(), 415, { worktreeName: "harness-stack", canRestack: true });
    expect(ids(menu)).toEqual([
      "checkout",
      "restackFrom",
      "push",
      "moveUp",
      "moveDown",
      "insertBelow",
      "split",
      "fold",
      "rename",
      "remove",
    ]);
    expect(menu).toContainEqual(expect.objectContaining({ id: "checkout", label: "Check out in harness-stack" }));
    expect(menu).toContainEqual(expect.objectContaining({ id: "fold", label: "Fold into #412" }));
    expect(menu).toContainEqual(expect.objectContaining({ id: "moveUp", shortcut: "Alt+Up" }));
    expect(menu).toContainEqual(expect.objectContaining({ id: "rename", shortcut: "F2" }));
  });

  it("can't fold the bottom PR or move the ends past the stack", () => {
    const bottom = branchMenu(myStack(), 412, { worktreeName: null, canRestack: true });
    expect(bottom).toContainEqual(expect.objectContaining({ id: "fold", disabled: true }));
    expect(bottom).toContainEqual(expect.objectContaining({ id: "moveDown", disabled: true }));
    const top = branchMenu(myStack(), 418, { worktreeName: null, canRestack: true });
    expect(top).toContainEqual(expect.objectContaining({ id: "moveUp", disabled: true }));
  });
});

describe("after a PR in the stack merges", () => {
  function merged(numbers: number[]): StackInput {
    const input = myStack();
    for (const pr of input.prs) {
      if (numbers.includes(pr.number)) {
        pr.merged = true;
        pr.open = false;
      }
    }
    return input;
  }

  it("offers to restack the rest and delete the merged git branch", () => {
    expect(mergeFollowUp(merged([412]), [412], true, new Set())).toEqual({
      kind: "restack",
      merged: [412],
      rest: [415, 418],
      branches: ["feat/driver-trait"],
    });
  });

  it("offers removing the worktree once the last PR merged, and stays away after Later", () => {
    expect(mergeFollowUp(merged([412, 415, 418]), [], true, new Set())).toEqual({ kind: "cleanup" });
    expect(mergeFollowUp(merged([412]), [412], true, new Set(["7:412"]))).toBeNull();
    expect(mergeFollowUp(myStack(), [], true, new Set())).toBeNull();
  });

  it("puts a dot on the worktree that holds the stack", () => {
    const listing = deskStacks(MAIN);
    listing.stacks[0].prs[0].merged = true;
    const worktree = { path: "/w/harness-stack", branch: "feat/claude-driver" } as never;
    const elsewhere = { path: "/w/other", branch: "jo-r/focus-trap" } as never;
    expect(followUpWorktrees(listing, [worktree, elsewhere], new Set())).toEqual(["/w/harness-stack"]);
    expect(followUpWorktrees(listing, [worktree], new Set(["7:412"]))).toEqual([]);
  });
});

describe("the behind badge", () => {
  it("names the base and the count, and hides when level", () => {
    expect(behindLabel("main", 3)).toBe("Behind main by 3");
    expect(behindLabel("main", 0)).toBeNull();
    expect(behindLabel("main", null)).toBeNull();
  });
});

describe("how far a PR is behind its base", () => {
  it("checks only the viewer's PRs, each against GitHub's base tip", () => {
    const inbox = deskInbox(MAIN);
    const inputs = myBaseInputs(inbox);
    const numbers = inputs.map((input) => input.number);
    expect(numbers).toEqual(expect.arrayContaining([128, 412, 415, 418]));
    expect(numbers).not.toContain(401);
    const top = inbox.prs.find((pr) => pr.number === 418);
    expect(inputs.find((input) => input.number === 418)).toEqual({
      number: 418,
      headSha: top?.headSha,
      baseRef: "feat/claude-driver",
      baseSha: top?.baseSha,
    });
  });

  it("names the PR below when the base is its git branch, and the base and count otherwise", () => {
    const inbox = deskInbox(MAIN);
    expect(driftLabel({ number: 418, baseRef: "feat/claude-driver" }, 4, inbox)).toBe("Behind #415");
    expect(driftLabel({ number: 128, baseRef: "main" }, 2, inbox)).toBe("Behind main by 2");
    expect(driftLabel({ number: 418, baseRef: "feat/claude-driver" }, 0, inbox)).toBeNull();
    expect(driftLabel({ number: 128, baseRef: "main" }, null, inbox)).toBeNull();
  });

  it("does not take a fork's git branch of the same name for the PR below", () => {
    const inbox = deskInbox(MAIN);
    const fork = { ...inbox.prs[0], number: 999, headRef: "main", headRepo: "someone/codex-custom" };
    expect(driftLabel({ number: 128, baseRef: "main" }, 2, { ...inbox, prs: [...inbox.prs, fork] })).toBe(
      "Behind main by 2",
    );
  });
});
