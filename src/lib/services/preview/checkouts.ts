// Checkout steps and PR links for the browser preview and the desk UI tests:
// PR worktrees and thread links held in memory, with the same
// step plan the backend's `review/desk/steps.rs` makes. Every PR starts with
// no checkout; `resetPreviewCheckouts` puts the preview back there.

import { DESK_ACTION_STEPS } from "$lib/bindings";
import type { CheckoutStep, CheckoutTarget, PrCheckout, PrLink, RepositorySettings, StepPlan } from "$lib/types";
import type { PreviewCommands } from "./commands";
import { DESK_VIEWER, deskInbox } from "./desk";
import { previewPrs } from "./fixtures";
import { repositoryCommands } from "./repositories";

const HOME = "/Users/ciaran/.codex";
const RANK: Record<CheckoutStep, number> = { noCheckout: 0, worktree: 1 };

const state = {
  checkouts: new Map<string, PrCheckout>(),
  links: new Map<string, PrLink>(),
};

const key = (repoDir: string, number: number) => `${repoDir}#${number}`;
const repoName = (repoDir: string) => repoDir.replace(/\/+$/, "").split("/").pop() || "repository";
const slug = (name: string) =>
  name
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");

/** The repository's settings, as the Add repository fakes store them. */
function settingsFor(repoDir: string): RepositorySettings {
  return repositoryCommands.reviewRepositorySettings?.(repoDir) as RepositorySettings;
}

/** Whether the PR's worktree goes on its real branch, as `placement` decides. */
function ownBranch(repoDir: string, target: CheckoutTarget): boolean {
  return target.stack !== null || (target.mine && target.headRepo === deskInbox(repoDir).repo);
}

function worktreePath(repoDir: string, target: CheckoutTarget): string {
  return ownBranch(repoDir, target)
    ? `${HOME}/worktrees/${repoName(repoDir)}/${slug(target.headRef)}`
    : `${HOME}/pr-worktrees/${repoName(repoDir)}/pr-${target.number}`;
}

function checkoutOf(repoDir: string, number: number): PrCheckout {
  return (
    state.checkouts.get(key(repoDir, number)) ?? {
      number,
      step: "noCheckout",
      path: null,
      headSha: null,
      gitBranch: null,
      setupPending: null,
      ahead: 0,
      behind: 0,
      dirty: false,
      managed: false,
    }
  );
}

function put(repoDir: string, checkout: PrCheckout): PrCheckout {
  state.checkouts.set(key(repoDir, checkout.number), checkout);
  return { ...checkout };
}

/** Seed a PR's checkout, for tests. */
export function setPreviewCheckout(repoDir: string, checkout: PrCheckout): void {
  put(repoDir, checkout);
}

/** Seed a thread's PR link, for tests. */
export function setPreviewPrLink(link: PrLink): void {
  state.links.set(link.threadId, { ...link });
}

export function resetPreviewCheckouts(): void {
  state.checkouts.clear();
  state.links.clear();
}

export const previewCheckoutCommands: PreviewCommands = {
  reviewCheckoutState: (repoDir, target) => ({ ...checkoutOf(repoDir, target.number) }),
  reviewPlanStep(repoDir, target, action): StepPlan {
    const current = checkoutOf(repoDir, target.number);
    const settings = settingsFor(repoDir);
    const needed = DESK_ACTION_STEPS[action];
    const stepUp = RANK[needed] > RANK[current.step] ? needed : null;
    const plan: StepPlan = {
      current: { ...current },
      stepUp,
      confirm: null,
      confirmPush: action === "push" && !target.mine,
      refused: null,
      worktreePath: current.path ?? worktreePath(repoDir, target),
    };
    const followsPolicy = action === "agentReviewWithTests" || action === "runLocally" || action === "openInEditor";
    if (stepUp === "worktree" && followsPolicy) {
      if (settings.reviewCheckoutPolicy === "never") plan.refused = "PR worktrees are turned off for this repository";
      else if (settings.reviewCheckoutPolicy === "ask" && !settings.skipCheckoutConfirm)
        plan.confirm = "createWorktree";
    } else if (stepUp === "worktree") {
      plan.confirm = "createWorktree";
    } else if (action === "push" && !target.mine) {
      plan.confirm = "pushToTheirBranch";
    }
    return plan;
  },
  reviewEnsurePrWorktree(repoDir, target) {
    const current = checkoutOf(repoDir, target.number);
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
    });
  },
  reviewRunCheckoutSetup(repoDir, number) {
    const current = checkoutOf(repoDir, number);
    put(repoDir, { ...current, setupPending: null });
  },
  reviewRemovePrWorktree(repoDir, number) {
    state.checkouts.delete(key(repoDir, number));
  },
  reviewPushToPr(repoDir, number) {
    put(repoDir, { ...checkoutOf(repoDir, number), ahead: 0 });
  },
  reviewManagedWorktreeAt(dir) {
    for (const [name, checkout] of state.checkouts) {
      if (checkout.path === dir && checkout.managed) {
        return { repoDir: name.slice(0, name.lastIndexOf("#")), number: checkout.number };
      }
    }
    return null;
  },
  reviewPruneClosed: () => ({ removedCheckouts: [], closed: [] }),
  reviewSetCheckoutPolicy(repoDir, policy, skipConfirm) {
    repositoryCommands.reviewSaveRepositorySettings?.(repoDir, {
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
    state.links.set(threadId, link);
    return { ...link };
  },
  reviewPrLinks: () => [...state.links.values()].map((link) => ({ ...link })),
  reviewMoveThreadToPr(threadId, number) {
    const link = state.links.get(threadId);
    if (!link) throw new Error("This thread is not filed under a pull request");
    if (number === null) {
      state.links.delete(threadId);
      return;
    }
    // The inbox's PRs, or the older picker's list that `reviewListPrs` serves.
    const pr = [...deskInbox(link.repoKey).prs, ...previewPrs].find((candidate) => candidate.number === number);
    if (!pr) throw new Error(`#${number} is not an open pull request`);
    state.links.set(threadId, { ...link, prNumber: number, mine: pr.author === DESK_VIEWER, closedAt: null });
  },
  reviewBranchProgress(threadId) {
    const link = state.links.get(threadId);
    return { startSha: link?.startSha ?? "", tipSha: link?.startSha ?? null, ahead: 0, behind: 0 };
  },
  reviewCheckOutThreadCommit: () => {},
};
