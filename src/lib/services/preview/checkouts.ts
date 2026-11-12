// Checking PRs out, and PR links, for the browser preview and the desk UI
// tests: what the backend's `review/desk/check_out.rs` does, on the preview
// desk. A checkout puts its worktree in the preview's worktree listing
// (`previewWorktrees`), so the sidebar shows it, and Undo takes it out again.

import { stackName } from "$lib/stack/stack";
import type {
  CheckOutOutcome,
  InboxPr,
  PrCheckout,
  PrLink,
  PrStack,
  RepositorySettings,
  WorktreeContext,
} from "$lib/types";
import type { DeskCommands } from "./commands";
import { desk, deskRepo, liveRepo } from "./desk";
import { prCheckout, worktreeEntry } from "./factories";
import { previewWorktrees } from "./fixtures";
import { repositoryCommands } from "./repositories";
import { setStackWorktree, stackWorktree } from "./stack";

/** The lock every worktree Pingex makes carries (`PR_WORKTREE_LOCK`). */
const LOCK = "pingex pr worktree";

const key = (repoDir: string, number: number) => `${repoDir}#${number}`;

/** A path segment from a git branch or folder name, as `check_out.rs` makes
 *  it: lower case, `[a-z0-9._-]`, every other character a `-`. */
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

/** Where every new PR or stack worktree goes, as `new_worktree_path` says. */
function worktreeDir(repoDir: string): string {
  return settingsFor(repoDir).worktreeDir ?? `${desk().home}/worktrees/${repoName(repoDir)}`;
}

function prOf(repoDir: string, number: number): InboxPr {
  const pr = liveRepo(repoDir).inbox.prs.find((candidate) => candidate.number === number);
  if (!pr) throw new Error(`#${number} is not an open pull request`);
  return pr;
}

/** A lone PR's branch: its own when it is mine and in this repository. */
function loneBranch(repoDir: string, pr: InboxPr): string {
  const slug = deskRepo(repoDir)?.slug.toLowerCase();
  const own = pr.author === desk().viewer && !!slug && pr.headRepo?.toLowerCase() === slug;
  return own ? pr.headRef : `pr/${pr.number}`;
}

/** The open stack PR `number` is an open member of. */
function stackOf(repoDir: string, number: number): PrStack | undefined {
  return liveRepo(repoDir).stacks.stacks.find(
    (stack) =>
      stack.open && stack.prs.some((entry) => entry.number === number && entry.state === "open" && !entry.merged),
  );
}

const openPrs = (stack: PrStack) => stack.prs.filter((entry) => entry.state === "open" && !entry.merged);

function rows(repoDir: string): PrCheckout[] {
  return [...desk().checkouts.entries()].filter(([name]) => name.startsWith(`${repoDir}#`)).map(([, row]) => row);
}

function put(repoDir: string, checkout: PrCheckout): PrCheckout {
  desk().checkouts.set(key(repoDir, checkout.number), checkout);
  return { ...checkout };
}

function addWorktree(path: string, branch: string): void {
  previewWorktrees.push(
    worktreeEntry({
      path,
      head: null,
      branch,
      locked: true,
      lockReason: LOCK,
      isManagedPrWorktree: /^pr\/\d+$/.test(branch),
    }),
  );
}

/** A worktree listed on one of `branches`, as `branch_in_use` finds it. */
function inUse(branches: string[]) {
  return previewWorktrees.find((entry) => !entry.missingDir && entry.branch && branches.includes(entry.branch));
}

function checkedOut(checkout: PrCheckout, created: boolean, createdBranches: string[] = []): CheckOutOutcome {
  return { kind: "checkedOut", checkout, created, createdBranches, kept: [], localStackWarning: null };
}

/** Every open PR of `stack` checked out at `path`, the worktree on `current`. */
function putStack(
  repoDir: string,
  stack: PrStack,
  path: string,
  current: string,
  extra: Partial<PrCheckout>,
): PrCheckout[] {
  return openPrs(stack).map((entry) =>
    put(
      repoDir,
      prCheckout(entry.number, {
        ...extra,
        path,
        headSha: entry.headSha,
        gitBranch: current,
        onPrBranch: entry.headRef === current,
        stack: stack.number,
      }),
    ),
  );
}

function checkOutStack(repoDir: string, stack: PrStack): CheckOutOutcome {
  const open = openPrs(stack);
  const top = open.at(-1);
  if (!top) throw new Error("This stack has no open pull requests");
  const existing = rows(repoDir).find((row) => row.stack === stack.number);
  if (!existing) {
    const found = inUse(open.map((entry) => entry.headRef));
    if (found) {
      const number = open.find((entry) => entry.headRef === found.branch)?.number ?? top.number;
      return { kind: "branchInUse", number, branch: found.branch ?? "", path: found.path, mainClone: found.isMain };
    }
  }
  const path = existing?.path ?? `${worktreeDir(repoDir)}/${segment(top.headRef)}`;
  if (!existing) {
    addWorktree(path, top.headRef);
    setStackWorktree(repoDir, top.number, { path, branch: top.headRef, clean: true });
  }
  const current = stackWorktree(repoDir, top.number)?.branch ?? top.headRef;
  const made = putStack(repoDir, stack, path, current, {
    adopted: existing?.adopted ?? false,
    setupPending: existing ? existing.setupPending : settingsFor(repoDir).setupCommand,
  });
  const checkout = made.find((row) => row.number === top.number) ?? made[0];
  return checkedOut(checkout, !existing, existing ? [] : open.map((entry) => entry.headRef));
}

/** What an agent starting in a worktree is told, as `worktree_context.rs` writes it. */
function contextAt(dir: string): WorktreeContext | null {
  for (const [name, row] of desk().checkouts) {
    if (row.path !== dir) continue;
    const repoDir = name.slice(0, name.lastIndexOf("#"));
    if (row.stack === null) {
      const branch = row.gitBranch ?? "";
      return {
        purpose: { kind: "managedPr", repoDir, number: row.number },
        text: branch.startsWith("pr/")
          ? `This directory is the worktree for pull request #${row.number}.\nThe PR is someone else's or comes from a fork, so it is on the local branch \`${branch}\`, which has no upstream: commits stay local. Don't push and don't set an upstream: the user pushes to the PR from Pingex.`
          : `This directory is the worktree for pull request #${row.number}.\nIt is on the PR's own git branch \`${branch}\`, which tracks the PR's branch on GitHub. Don't push unless asked: a push updates the PR.`,
        position: null,
      };
    }
    const stack = liveRepo(repoDir).stacks.stacks.find((candidate) => candidate.number === row.stack);
    if (!stack) return null;
    const open = openPrs(stack);
    const at = open.findIndex((entry) => entry.headRef === row.gitBranch);
    const titled = (number: number) => liveRepo(repoDir).inbox.prs.find((pr) => pr.number === number)?.title ?? "";
    return {
      purpose: {
        kind: "stack",
        repoDir,
        number: stack.number,
        name: stackName(stack),
        baseRef: stack.baseRef,
        prs: open.map((entry) => ({ number: entry.number, title: titled(entry.number), headRef: entry.headRef })),
        localStack: false,
      },
      text: `This directory is the worktree of GitHub stack #${stack.number} "${stackName(stack)}".`,
      position: at === -1 ? null : { position: at + 1, total: open.length },
    };
  }
  return null;
}

export const checkoutCommands = {
  reviewCheckOut(repoDir, number) {
    const stack = stackOf(repoDir, number);
    if (stack) return checkOutStack(repoDir, stack);
    const existing = desk().checkouts.get(key(repoDir, number));
    if (existing) return checkedOut({ ...existing }, false);
    const pr = prOf(repoDir, number);
    const branch = loneBranch(repoDir, pr);
    const found = inUse([branch]);
    if (found) return { kind: "branchInUse", number, branch, path: found.path, mainClone: found.isMain };
    const path = `${worktreeDir(repoDir)}/${segment(branch)}`;
    addWorktree(path, branch);
    const checkout = put(
      repoDir,
      prCheckout(number, {
        path,
        headSha: pr.headSha,
        gitBranch: branch,
        setupPending: settingsFor(repoDir).setupCommand,
      }),
    );
    return checkedOut(checkout, true);
  },
  reviewCheckOutStack(repoDir, number) {
    const stack = liveRepo(repoDir).stacks.stacks.find((candidate) => candidate.number === number && candidate.open);
    if (!stack) throw new Error("That stack is not open on GitHub");
    return checkOutStack(repoDir, stack);
  },
  reviewAdoptWorktree(repoDir, number, path) {
    const entry = previewWorktrees.find((candidate) => candidate.path === path);
    if (!entry?.branch) throw new Error(`${path} is not a worktree of this repository`);
    const stack = stackOf(repoDir, number);
    if (stack) {
      setStackWorktree(repoDir, number, { path, branch: entry.branch, clean: true });
      const made = putStack(repoDir, stack, path, entry.branch, { adopted: true, mainCheckout: entry.isMain });
      return made.find((row) => row.number === number) ?? made[0];
    }
    return put(
      repoDir,
      prCheckout(number, {
        path,
        headSha: prOf(repoDir, number).headSha,
        gitBranch: entry.branch,
        adopted: true,
        mainCheckout: entry.isMain,
      }),
    );
  },
  reviewSwitchStackBranch(repoDir, number) {
    const row = desk().checkouts.get(key(repoDir, number));
    const stack = row?.stack == null ? undefined : liveRepo(repoDir).stacks.stacks.find((s) => s.number === row.stack);
    if (!row || !stack) throw new Error(`#${number} is not checked out in a stack's worktree`);
    const branch = stack.prs.find((entry) => entry.number === number)?.headRef ?? "";
    if (row.adopted)
      throw new Error(`Pingex doesn't switch a worktree it found. Switch ${row.path} to ${branch} there`);
    if (row.dirty) throw new Error(`${row.path} has uncommitted changes. Commit or stash them, then switch`);
    setStackWorktree(repoDir, number, { path: row.path, branch, clean: true });
    const listed = previewWorktrees.find((entry) => entry.path === row.path);
    if (listed) listed.branch = branch;
    const made = putStack(repoDir, stack, row.path, branch, { adopted: false, setupPending: row.setupPending });
    return made.find((entry) => entry.number === number) ?? made[0];
  },
  reviewPrCheckout: (repoDir, number) => {
    const row = desk().checkouts.get(key(repoDir, number));
    return row ? { ...row } : null;
  },
  reviewPrCheckouts: (repoDir) => rows(repoDir).map((row) => ({ ...row })),
  reviewRunCheckoutSetup(repoDir, number) {
    const path = desk().checkouts.get(key(repoDir, number))?.path;
    for (const row of rows(repoDir)) if (row.path === path) put(repoDir, { ...row, setupPending: null });
    return null;
  },
  reviewRemovePrWorktree(repoDir, number) {
    const row = desk().checkouts.get(key(repoDir, number));
    if (!row) return null;
    const together = rows(repoDir).filter((other) => other.path === row.path);
    if (!row.adopted) {
      if (together.some((other) => other.dirty || (other.onPrBranch && other.ahead > 0))) {
        throw new Error("This worktree has local changes or unpushed commits, so it was kept");
      }
      const index = previewWorktrees.findIndex((entry) => entry.path === row.path);
      if (index >= 0) previewWorktrees.splice(index, 1);
      if (row.stack !== null) setStackWorktree(repoDir, number, null);
    }
    for (const other of together) desk().checkouts.delete(key(repoDir, other.number));
    return null;
  },
  reviewPushToPr(repoDir, number) {
    const row = desk().checkouts.get(key(repoDir, number));
    if (row) put(repoDir, { ...row, ahead: 0 });
    return null;
  },
  reviewManagedWorktreeAt(dir) {
    for (const [name, row] of desk().checkouts) {
      if (row.path === dir && row.stack === null && !row.adopted && row.gitBranch?.startsWith("pr/")) {
        return { repoDir: name.slice(0, name.lastIndexOf("#")), number: row.number };
      }
    }
    return null;
  },
  reviewWorktreeContext: (dir) => contextAt(dir),
  reviewPruneClosed: () => ({ removedCheckouts: [], closed: [] }),
  reviewLinkThread(repoDir, threadId, number) {
    const row = desk().checkouts.get(key(repoDir, number));
    if (!row) throw new Error(`Check out #${number} first`);
    const pr = prOf(repoDir, number);
    const link: PrLink = {
      threadId,
      repoKey: repoDir,
      prNumber: number,
      mine: pr.author === desk().viewer,
      checkoutPath: row.path,
      gitBranch: row.gitBranch ?? pr.headRef,
      startSha: row.headSha ?? pr.headSha,
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
