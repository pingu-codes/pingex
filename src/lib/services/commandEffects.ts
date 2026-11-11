// What each git and review command changes, declared once per command. The
// `commands` proxy announces every settled one (`commandEvents.ts`), and
// everything that must catch up after Pingex's own writes derives from this
// table: the query cache marks what went stale, the inbox resyncs, the
// branch chips and the worktree listing re-read the directories named.
//
// The table is exhaustive over the generated git and review commands, so a
// new Rust command fails `bun run check` until it says what it changes.

import type { commands } from "$lib/bindings";
import { argNamed, onCommandSettled } from "$lib/services/commandEvents";
import type { ReadName } from "$lib/services/queryCache.svelte";

/** Something a command can change.
 *  - `git`: the working tree, refs or worktrees of the directories it names.
 *  - `github`: the repository's pull requests, on GitHub or in the clone's
 *    branches that follow them. A write here may fetch, push or rebase, so it
 *    implies `git`.
 *  - `inbox`: what the inbox lists for the repository (a review, a merge, a
 *    retarget, a stack edit).
 *  - `account`: which GitHub account answers for the repository it names, or
 *    for every repository when it names none.
 *  - `profile`: only the profile database (viewed marks, drafts, rules,
 *    settings, agent reviews, thread links).
 *  - `prRefs`: only the hidden PR refs (`refs/pingex/pr/*`), which reads take
 *    by SHA.
 *  - `clone`: a new clone, which nothing has read yet. */
export type Change = "git" | "github" | "inbox" | "account" | "profile" | "prRefs" | "clone";

export type CommandEffect = {
  /** Empty for a read. */
  readonly changes: readonly Change[];
  /** Cached reads that report what it wrote although `changes` does not
   *  imply them, in the repository it names (every one when it names none). */
  readonly stale?: readonly ReadName[];
};

/** The commands the proxy announces once they settle. */
export type AnnouncedCommand = Extract<keyof typeof commands, `git${string}` | `review${string}`>;

const READ: CommandEffect = { changes: [] };
const GIT: CommandEffect = { changes: ["git"] };
const GITHUB: CommandEffect = { changes: ["github"] };
/** A GitHub write the inbox shows, so it resyncs after one. */
const INBOX: CommandEffect = { changes: ["github", "inbox"] };
const PROFILE: CommandEffect = { changes: ["profile"] };

export const EFFECTS: Record<AnnouncedCommand, CommandEffect> = {
  gitBranches: READ,
  gitContext: READ,
  gitFileDiff: READ,
  gitRecentCommits: READ,
  gitRepoInfo: READ,
  gitStagedFileDiff: READ,
  gitSegmentDiff: READ,
  gitStatus: READ,
  gitThreadChanges: READ,
  gitWorktreeHandoffPreflight: READ,
  gitWorkingDiff: READ,
  gitWorktrees: READ,
  gitCheckoutBranch: GIT,
  gitCommit: GIT,
  gitCreateBranch: GIT,
  gitDiscard: GIT,
  gitFetch: GIT,
  gitPull: GIT,
  gitPush: GIT,
  gitStage: GIT,
  gitUnstage: GIT,
  gitWorktreeAdd: GIT,
  gitWorktreeAddTemp: GIT,
  gitWorktreeHandoff: GIT,
  gitWorktreeLock: GIT,
  gitWorktreePrune: GIT,
  gitWorktreeRemove: GIT,
  gitWorktreeUnlock: GIT,

  reviewActiveAgentReviews: READ,
  reviewAgentReviewPrompt: READ,
  reviewAgentReviews: READ,
  reviewBranchProgress: READ,
  // Measured from the reviewer's seen head, which no argument names, so it
  // is read directly rather than cached.
  reviewChangesSince: READ,
  reviewCheckFresh: READ,
  reviewCheckLogs: READ,
  reviewCheckoutState: READ,
  reviewDiff: READ,
  reviewFile: READ,
  reviewFindClones: READ,
  reviewFindings: READ,
  reviewGithubAccount: READ,
  reviewInbox: READ,
  reviewInspectFolder: READ,
  reviewListPrs: READ,
  reviewLoadDraft: READ,
  reviewLocalDiff: READ,
  reviewManagedWorktreeAt: READ,
  reviewMyRepositories: READ,
  reviewPendingReview: READ,
  reviewPlanStep: READ,
  reviewPrCheckouts: READ,
  reviewPrDetail: READ,
  reviewPrLinks: READ,
  reviewProviderStatus: READ,
  reviewPrsBehind: READ,
  reviewPrsOnlyRepositories: READ,
  reviewRepositoriesRequestingReview: READ,
  reviewRepository: READ,
  reviewRepositoryAccount: READ,
  reviewRepositorySettings: READ,
  reviewRestackPlan: READ,
  reviewRules: READ,
  reviewStackOverview: READ,
  reviewStacks: READ,
  reviewViewedFiles: READ,
  reviewWorktreeContext: READ,

  reviewAddStackBranch: INBOX,
  reviewClose: INBOX,
  reviewCreatePr: INBOX,
  reviewDeleteMergedBranches: INBOX,
  reviewFoldIntoParent: INBOX,
  reviewMarkReady: INBOX,
  reviewMerge: INBOX,
  reviewPushStack: INBOX,
  reviewRemoveFromStack: INBOX,
  reviewRenameStackBranch: INBOX,
  reviewReorderStack: INBOX,
  reviewReply: INBOX,
  reviewResolveThread: INBOX,
  reviewRetarget: INBOX,
  reviewSplitBranch: INBOX,
  reviewSubmit: INBOX,
  reviewSubmitPendingReview: INBOX,
  reviewAbortRestack: GITHUB,
  reviewContinueRestack: GITHUB,
  reviewDeleteComment: GITHUB,
  reviewEditComment: GITHUB,
  // Fetches the PR heads it makes branches of.
  reviewEnsureStackWorktree: GITHUB,
  reviewPushToPr: GITHUB,
  reviewRerunFailedChecks: GITHUB,
  reviewRestack: GITHUB,

  reviewCheckOutThreadCommit: GIT,
  reviewEnsurePrWorktree: GIT,
  reviewRemovePrWorktree: GIT,
  reviewRunCheckoutSetup: GIT,

  reviewSetRepositoryAccount: { changes: ["account"] },
  reviewSwitchGithubAccount: { changes: ["account"] },

  reviewAddPrsOnlyRepository: PROFILE,
  reviewAddRule: PROFILE,
  reviewAgentReviewStarted: PROFILE,
  reviewClaimAgentReviews: PROFILE,
  reviewDeleteDraft: PROFILE,
  reviewDiscardPendingReview: PROFILE,
  reviewEnqueueAgentReview: PROFILE,
  reviewForgetPr: PROFILE,
  reviewMarkActivitySeen: PROFILE,
  reviewMarkSeen: PROFILE,
  reviewMutePr: PROFILE,
  reviewRecordAgentReviewResult: PROFILE,
  reviewRemoveAgentReview: PROFILE,
  reviewRemovePrsOnlyRepository: PROFILE,
  reviewRemoveRule: PROFILE,
  reviewRerunAgentReview: PROFILE,
  reviewRerunOnNewCommits: PROFILE,
  reviewSaveDraft: PROFILE,
  reviewSavePendingReview: PROFILE,
  reviewSaveRepositorySettings: PROFILE,
  reviewSetCheckoutPolicy: PROFILE,
  reviewSetFindingState: PROFILE,
  reviewSetViewed: PROFILE,
  reviewUpdateRule: PROFILE,
  // Thread links and the branches they follow.
  reviewLinkThread: { changes: ["profile"], stale: ["reviewBranchProgress"] },
  reviewMoveThreadToPr: { changes: ["profile"], stale: ["reviewBranchProgress"] },
  reviewLinkClone: { changes: ["profile"], stale: ["reviewBranchProgress", "reviewProviderStatus"] },
  reviewFetchPrs: { changes: ["prRefs"] },
  // Runs after every complete inbox sync: closed PRs' refs, links and
  // managed worktrees.
  reviewPruneClosed: { changes: ["prRefs", "profile"], stale: ["reviewCheckoutState", "reviewPrCheckouts"] },
  reviewCloneRepository: { changes: ["clone"] },
};

/** Whether the proxy announces `name` once it settles. */
export function isAnnounced(name: string): name is AnnouncedCommand {
  return Object.hasOwn(EFFECTS, name);
}

/** The arguments a command names a directory with. */
const PATH_ARGUMENTS = ["dir", "repoDir", "worktreePath", "targetDir"] as const;

/** What one settled command changed, and where. */
export type Effect = {
  readonly command: AnnouncedCommand;
  /** With what each change implies (`github` brings `git`). */
  readonly changes: ReadonlySet<Change>;
  readonly stale: readonly ReadName[];
  /** The repository it names (`repoDir`), if any. */
  readonly repo: string | undefined;
  /** Every directory it names. */
  readonly paths: readonly string[];
};

/** Run `listener` after each settled command that changed something; reads
 *  are skipped. Returns the stop function. */
export function onEffect(listener: (effect: Effect) => void): () => void {
  return onCommandSettled((command) => {
    if (!isAnnounced(command.name)) return;
    const declared = EFFECTS[command.name];
    if (declared.changes.length === 0) return;
    const changes = new Set(declared.changes);
    if (changes.has("github")) changes.add("git");
    const repo = argNamed(command, "repoDir");
    listener({
      command: command.name,
      changes,
      stale: declared.stale ?? [],
      repo: typeof repo === "string" ? repo : undefined,
      paths: PATH_ARGUMENTS.flatMap((name) => {
        const path = argNamed(command, name);
        return typeof path === "string" ? [path] : [];
      }),
    });
  });
}
