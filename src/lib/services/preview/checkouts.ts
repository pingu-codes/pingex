// Checkout steps and PR links for the browser preview and the desk UI tests:
// PR worktrees and thread links on the preview desk, with the step plan the
// backend's `review/desk/steps.rs` makes. The recorded cases in
// `tests/fixtures/desk/step_plans.json` hold both to the same plans.

import { DESK_ACTION_STEPS } from "$lib/bindings";
import type {
  CheckoutStep,
  CheckoutTarget,
  PrCheckout,
  PrLink,
  RepositorySettings,
  StackWorktree,
  StepPlan,
} from "$lib/types";
import type { DeskCommands } from "./commands";
import { desk, deskRepo, liveRepo } from "./desk";
import { prCheckout } from "./factories";
import { repositoryCommands } from "./repositories";
import { setStackWorktree, stackWorktree } from "./stack";

const RANK: Record<CheckoutStep, number> = { noCheckout: 0, worktree: 1 };

const key = (repoDir: string, number: number) => `${repoDir}#${number}`;

/** A path segment from a git branch or folder name, as `steps.rs` makes it:
 *  lower case, `[a-z0-9._-]`, every other character a `-`. */
function segment(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9._-]/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "");
  return slug || "checkout";
}

const repoName = (repoDir: string) => segment(repoDir.replace(/\/+$/, "").split("/").pop() || "repository");

/** The repository's settings, as the Add repository fakes store them. */
function settingsFor(repoDir: string): RepositorySettings {
  return repositoryCommands.reviewRepositorySettings(repoDir);
}

/** Whether the PR's worktree goes on its real branch, as `placement` decides. */
function ownBranch(repoDir: string, target: CheckoutTarget): boolean {
  const slug = deskRepo(repoDir)?.slug.toLowerCase();
  return target.stack !== null || (target.mine && !!slug && target.headRepo?.toLowerCase() === slug);
}

function worktreePath(repoDir: string, target: CheckoutTarget): string {
  const { home } = desk();
  if (!ownBranch(repoDir, target)) return `${home}/pr-worktrees/${repoName(repoDir)}/pr-${target.number}`;
  const group = settingsFor(repoDir).worktreeDir ?? `${home}/worktrees/${repoName(repoDir)}`;
  return `${group}/${segment(target.headRef)}`;
}

function checkoutOf(repoDir: string, number: number): PrCheckout {
  return desk().checkouts.get(key(repoDir, number)) ?? prCheckout(number);
}

/** The worktree a stack neighbour of `target` made, on one of the stack's
 *  git branches, as `pr_checkouts::candidates` finds it. */
function neighbours(repoDir: string, target: CheckoutTarget): StackWorktree | null {
  for (const [name, checkout] of desk().checkouts) {
    if (!name.startsWith(`${repoDir}#`) || checkout.number === target.number || !checkout.path) continue;
    if (checkout.gitBranch && target.stackBranches?.includes(checkout.gitBranch))
      return { path: checkout.path, branch: checkout.gitBranch, clean: !checkout.dirty };
  }
  return null;
}

/** Where `target` is, as `pr_checkouts::locate` finds it: its own worktree, or
 *  its stack's, which may be on another of the stack's git branches. */
function located(repoDir: string, target: CheckoutTarget): PrCheckout {
  const own = checkoutOf(repoDir, target.number);
  const shared =
    target.stack === null
      ? null
      : (stackWorktree(repoDir, target.number) ?? (own.path ? null : neighbours(repoDir, target)));
  if (!shared) return own;
  if (own.path && own.path !== shared.path) return own;
  return {
    ...own,
    step: "worktree",
    path: shared.path,
    headSha: target.headSha,
    gitBranch: shared.branch,
    dirty: !shared.clean,
    onPrBranch: shared.branch === target.headRef,
  };
}

function put(repoDir: string, checkout: PrCheckout): PrCheckout {
  desk().checkouts.set(key(repoDir, checkout.number), checkout);
  return { ...checkout };
}

export const checkoutCommands = {
  reviewCheckoutState: (repoDir, target) => located(repoDir, target),
  reviewPrCheckouts: (repoDir, targets) =>
    targets.map((target) => located(repoDir, target)).filter((checkout) => checkout.path !== null),
  reviewEnsureStackWorktree(repoDir, input) {
    const open = input.prs.filter((pr) => pr.open && !pr.merged);
    const top = open.at(-1);
    if (!top) throw new Error("This stack has no open pull requests");
    const created = stackWorktree(repoDir, top.number) === null;
    const path = `${settingsFor(repoDir).worktreeDir ?? `${desk().home}/worktrees/${repoName(repoDir)}`}/${segment(top.headRef)}`;
    if (created) setStackWorktree(repoDir, top.number, { path, branch: top.headRef, clean: true });
    const target: CheckoutTarget = {
      number: top.number,
      headRef: top.headRef,
      headSha: top.headSha,
      mine: false,
      stack: input.name,
      headRepo: null,
      stackBranches: open.map((pr) => pr.headRef),
    };
    return {
      checkout: located(repoDir, target),
      created,
      createdBranches: created ? open.map((pr) => pr.headRef) : [],
      kept: [],
    };
  },
  reviewPlanStep(repoDir, target, action): StepPlan {
    const current = located(repoDir, target);
    const settings = settingsFor(repoDir);
    const needed = DESK_ACTION_STEPS[action];
    const stepUp = RANK[needed] > RANK[current.step] ? needed : null;
    const switchBranch = needed === "worktree" && target.stack !== null && current.path !== null && !current.onPrBranch;
    const plan: StepPlan = {
      current: { ...current },
      stepUp,
      confirm: null,
      confirmPush: action === "push" && !target.mine,
      refused: null,
      worktreePath: current.path ?? worktreePath(repoDir, target),
      switchBranch,
    };
    const followsPolicy = action === "agentReviewWithTests" || action === "runLocally" || action === "openInEditor";
    if (stepUp === "worktree" && followsPolicy) {
      if (settings.reviewCheckoutPolicy === "never") plan.refused = "PR worktrees are turned off for this repository";
      else if (settings.reviewCheckoutPolicy === "ask" && !settings.skipCheckoutConfirm)
        plan.confirm = "createWorktree";
    } else if (stepUp === "worktree") {
      plan.confirm = "createWorktree";
    } else if (switchBranch) {
      plan.confirm = "switchBranch";
    } else if (action === "push" && !target.mine) {
      plan.confirm = "pushToTheirBranch";
    }
    return plan;
  },
  reviewEnsurePrWorktree(repoDir, target) {
    const current = located(repoDir, target);
    if (current.path && target.stack !== null && !current.onPrBranch) {
      if (current.dirty) throw new Error(`${current.path} has uncommitted changes. Commit or stash them, then switch`);
      setStackWorktree(repoDir, target.number, { path: current.path, branch: target.headRef, clean: true });
      return put(repoDir, { ...current, gitBranch: target.headRef, onPrBranch: true });
    }
    if (current.step !== "noCheckout") return { ...current };
    return put(repoDir, {
      number: target.number,
      step: "worktree",
      path: worktreePath(repoDir, target),
      headSha: target.headSha,
      gitBranch: ownBranch(repoDir, target) ? target.headRef : `pr/${target.number}`,
      setupPending: settingsFor(repoDir).setupCommand,
      ahead: 0,
      behind: 0,
      dirty: false,
      managed: !ownBranch(repoDir, target),
      onPrBranch: true,
      mainCheckout: false,
    });
  },
  reviewRunCheckoutSetup(repoDir, number) {
    const current = checkoutOf(repoDir, number);
    put(repoDir, { ...current, setupPending: null });
    return null;
  },
  reviewRemovePrWorktree(repoDir, number) {
    desk().checkouts.delete(key(repoDir, number));
    return null;
  },
  reviewPushToPr(repoDir, number) {
    put(repoDir, { ...checkoutOf(repoDir, number), ahead: 0 });
    return null;
  },
  reviewManagedWorktreeAt(dir) {
    for (const [name, checkout] of desk().checkouts) {
      if (checkout.path === dir && checkout.managed) {
        return { repoDir: name.slice(0, name.lastIndexOf("#")), number: checkout.number };
      }
    }
    return null;
  },
  reviewWorktreeContext(dir) {
    for (const [name, checkout] of desk().checkouts) {
      if (checkout.path !== dir || !checkout.managed) continue;
      const number = checkout.number;
      return {
        purpose: { kind: "managedPr", repoDir: name.slice(0, name.lastIndexOf("#")), number },
        text: `This directory is a worktree for pull request #${number}, which is someone else's or comes from a fork. Pingex made it and removes it once the PR has closed and it holds no work.\nIt is on the local branch \`pr/${number}\`, which has no upstream, so commits stay local. Don't push and don't set an upstream: the user pushes to the PR from Pingex.`,
      };
    }
    return null;
  },
  reviewPruneClosed: () => ({ removedCheckouts: [], closed: [] }),
  reviewSetCheckoutPolicy(repoDir, policy, skipConfirm) {
    return repositoryCommands.reviewSaveRepositorySettings(repoDir, {
      ...settingsFor(repoDir),
      reviewCheckoutPolicy: policy,
      skipCheckoutConfirm: skipConfirm,
    });
  },
  reviewLinkThread(repoDir, threadId, target) {
    const checkout = checkoutOf(repoDir, target.number);
    const link: PrLink = {
      threadId,
      repoKey: repoDir,
      prNumber: target.number,
      mine: target.mine,
      step: checkout.step,
      checkoutPath: checkout.path,
      gitBranch: checkout.gitBranch ?? target.headRef,
      startSha: checkout.headSha ?? target.headSha,
      createdAt: Math.floor(Date.now() / 1000),
      closedAt: null,
    };
    desk().links.set(threadId, link);
    return { ...link };
  },
  reviewPrLinks: () => [...desk().links.values()].map((link) => ({ ...link })),
  reviewMoveThreadToPr(threadId, number) {
    const link = desk().links.get(threadId);
    if (!link) throw new Error("This thread is not filed under a pull request");
    if (number === null) {
      desk().links.delete(threadId);
      return null;
    }
    const pr = liveRepo(link.repoKey).inbox.prs.find((candidate) => candidate.number === number);
    if (!pr) throw new Error(`#${number} is not an open pull request`);
    desk().links.set(threadId, { ...link, prNumber: number, mine: pr.author === desk().viewer, closedAt: null });
    return null;
  },
  reviewBranchProgress(threadId) {
    const link = desk().links.get(threadId);
    return { startSha: link?.startSha ?? "", tipSha: link?.startSha ?? null, ahead: 0, behind: 0 };
  },
  reviewCheckOutThreadCommit: () => null,
} satisfies Partial<DeskCommands>;
