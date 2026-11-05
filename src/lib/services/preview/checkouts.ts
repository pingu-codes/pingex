// Checkout steps and PR links for the browser preview and the desk UI tests:
// review checkouts, worktrees and thread links held in memory, with the same
// step plan the backend's `review/desk/steps.rs` makes. Every PR starts with
// no checkout; `resetPreviewCheckouts` puts the preview back there.

import { DESK_ACTION_STEPS } from "$lib/bindings";
import type { CheckoutStep, CheckoutTarget, PrCheckout, PrLink, RepositorySettings, StepPlan } from "$lib/types";
import type { PreviewCommands } from "./commands";
import { DESK_VIEWER, deskInbox } from "./desk";

const HOME = "/Users/ciaran/.codex";
const RANK: Record<CheckoutStep, number> = { noCheckout: 0, reviewCheckout: 1, worktree: 2 };

const state = {
  checkouts: new Map<string, PrCheckout>(),
  settings: new Map<string, RepositorySettings>(),
  links: new Map<string, PrLink>(),
};

const key = (repoDir: string, number: number) => `${repoDir}#${number}`;
const repoName = (repoDir: string) => repoDir.replace(/\/+$/, "").split("/").pop() || "repository";
const slug = (name: string) =>
  name
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");

function settingsFor(repoDir: string): RepositorySettings {
  return (
    state.settings.get(repoDir) ?? {
      repoKey: repoDir,
      setupCommand: "bun install",
      reviewCheckoutPolicy: "ask",
      skipCheckoutConfirm: false,
      worktreeDir: null,
      prSource: "origin",
      updatedAt: 0,
    }
  );
}

function reviewCheckoutPath(repoDir: string, target: CheckoutTarget): string {
  const name = target.stack ? `stack-${slug(target.stack)}` : `pr-${target.number}`;
  return `${HOME}/review-checkouts/${repoName(repoDir)}/${name}`;
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
      behindHead: false,
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
  state.settings.clear();
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
      refused: null,
      reviewCheckoutPath: reviewCheckoutPath(repoDir, target),
    };
    if (stepUp === "reviewCheckout") {
      if (settings.reviewCheckoutPolicy === "never")
        plan.refused = "Review checkouts are turned off for this repository";
      else if (settings.reviewCheckoutPolicy === "ask" && !settings.skipCheckoutConfirm)
        plan.confirm = "createReviewCheckout";
    } else if (stepUp === "worktree") {
      plan.confirm = current.step === "reviewCheckout" ? "promoteOrCreateWorktree" : "createWorktree";
    } else if (action === "push" && !target.mine) {
      plan.confirm = "pushToTheirBranch";
    }
    return plan;
  },
  reviewEnsureReviewCheckout(repoDir, target) {
    const current = checkoutOf(repoDir, target.number);
    if (current.step !== "noCheckout") return { ...current };
    return put(repoDir, {
      number: target.number,
      step: "reviewCheckout",
      path: reviewCheckoutPath(repoDir, target),
      headSha: target.headSha,
      gitBranch: null,
      setupPending: settingsFor(repoDir).setupCommand,
      behindHead: false,
    });
  },
  reviewRunCheckoutSetup(repoDir, number) {
    const current = checkoutOf(repoDir, number);
    put(repoDir, { ...current, setupPending: null });
  },
  reviewPromoteCheckout: (repoDir, target) =>
    put(repoDir, { ...checkoutOf(repoDir, target.number), step: "worktree", gitBranch: target.headRef }),
  reviewCreatePrWorktree: (repoDir, target, path) =>
    put(repoDir, {
      number: target.number,
      step: "worktree",
      path: path ?? `${HOME}/worktrees/${repoName(repoDir)}/${slug(target.headRef)}`,
      headSha: target.headSha,
      gitBranch: target.headRef,
      setupPending: null,
      behindHead: false,
    }),
  reviewRemoveReviewCheckout(repoDir, number) {
    state.checkouts.delete(key(repoDir, number));
  },
  reviewPruneClosed: () => ({ removedCheckouts: [], closed: [] }),
  reviewRepositorySettings: (repoDir) => ({ ...settingsFor(repoDir) }),
  reviewSetCheckoutPolicy(repoDir, policy, skipConfirm) {
    state.settings.set(repoDir, {
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
    const pr = deskInbox(link.repoKey).prs.find((candidate) => candidate.number === number);
    if (!pr) throw new Error(`#${number} is not an open pull request`);
    state.links.set(threadId, { ...link, prNumber: number, mine: pr.author === DESK_VIEWER, closedAt: null });
  },
  reviewBranchProgress(threadId) {
    const link = state.links.get(threadId);
    return { startSha: link?.startSha ?? "", tipSha: link?.startSha ?? null, ahead: 0, behind: 0 };
  },
  reviewCheckOutThreadCommit: () => {},
};
