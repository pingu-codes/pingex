// The browser preview's review desk: one in-memory owner of everything the
// review command fakes read and write — repositories and their PRs, stacks,
// PR worktrees and thread links, pending reviews, viewed marks, the inbox's
// seen state, agent reviews, and the GitHub account and folders Add
// repository finds. It stands in for the Rust `ReviewDesk`
// (`src-tauri/src/review/desk/`) when the page is not under Tauri.
//
// It starts from the demo (`./demoDesk`). `seedPreviewDesk` replaces it with
// exactly what a test states, built with `./factories`; `resetPreviewDesk`
// goes back to the demo, and the vitest setup calls it before every test.
// The fakes for each area live beside this file and keep no state of their
// own.

import type {
  AgentReview,
  CommentRef,
  CommitLine,
  FeedEntry,
  FileBlob,
  Finding,
  FolderFacts,
  GitHubAccount,
  GitHubPerson,
  GitHubRepo,
  Inbox,
  InboxEvent,
  InboxPr,
  KnownClone,
  NeedsMe,
  PendingComment,
  PendingReview,
  PrCheckout,
  PrComment,
  PrDetail,
  PrLink,
  PrTriage,
  Repository,
  RepositorySettings,
  RestackProgress,
  ReviewDraft,
  ReviewRequests,
  ReviewRule,
  StackListing,
  StackWorktree,
} from "$lib/types";
import type { DeskCommands } from "./commands";
import { demoDesk } from "./demoDesk";
import { largeDiffFiles, NO_ANCHOR, prDetail, prSummary } from "./factories";

// --- what a seed states -------------------------------------------------------

/** A stack as GitHub lists it, plus how the stack fakes answer for it. */
export type StackSeed = {
  /** GitHub's stack number. */
  number: number;
  baseRef?: string;
  /** Its PRs' numbers, bottom first; each is one of the repository's PRs. */
  prs: number[];
  /** PRs of the stack that merged; they stay in GitHub's listing, closed. */
  merged?: number[];
  /** Commits the base gained since the stack forked. */
  behind?: number;
  /** Open PRs no longer on the head of the PR below them. */
  drifted?: number[];
  worktree?: StackWorktree | null;
  /** The first restack that moves stops here until continued. */
  conflict?: { at: number; files: string[] } | null;
  /** Commits on each PR's git branch; 1 when not given. */
  commits?: Record<number, number>;
  /** What the base gained, newest first; made up from `behind` when not given. */
  baseCommits?: CommitLine[];
};

/** One repository on the desk and what GitHub and the clone say about it. */
export type RepositorySeed = {
  repository: Repository;
  /** Its open PRs, as the inbox lists them. */
  prs?: InboxPr[];
  stacks?: StackSeed[];
  /** A PR's detail page; a PR without one gets `prDetail(pr)`. */
  details?: PrDetail[];
  /** "Since you last looked", newest first. */
  feed?: Omit<FeedEntry, "id">[];
  people?: GitHubPerson[];
  events?: InboxEvent[];
  syncedAt?: number;
  checkouts?: PrCheckout[];
  settings?: Partial<RepositorySettings>;
  /** Commits since my latest review, which the backend counts in the clone. */
  commitsSinceMyReview?: Record<number, number>;
  /** How far PRs in no stack are behind their base. */
  behind?: Record<number, number>;
};

export type DeskSeed = {
  /** Who the desk reads GitHub as; `ciaran`, whom `inboxPr` makes the author. */
  viewer?: string;
  /** The Home that PR worktrees go under. */
  home?: string;
  repositories?: RepositorySeed[];
  links?: PrLink[];
  agentReviews?: AgentReview[];
  findings?: Finding[];
  /** The gh account and what it can see; signed in as the viewer when not given. */
  github?: {
    account?: GitHubAccount;
    repositories?: GitHubRepo[];
    /** Review requests in repositories not on the desk. */
    requests?: ReviewRequests[];
    knownClones?: KnownClone[];
  };
  /** Folders on disk Add repository can inspect, beyond the desk's clones. */
  folders?: Omit<FolderFacts, "prSource">[];
};

// --- the state ------------------------------------------------------------------

export type StackSim = Required<Omit<StackSeed, "number" | "prs" | "merged" | "baseRef" | "commits">> & {
  commits: Map<number, number>;
  run: RestackProgress | null;
};

/** One repository's live data, edited in place by the fakes. */
export type RepoData = {
  /** The inbox as synced; `triage`, `counts` and `feed` are worked out per read. */
  inbox: Inbox;
  stacks: StackListing;
  sims: Map<number, StackSim>;
  details: Map<number, PrDetail>;
  feed: FeedEntry[];
  muted: Map<number, string>;
  commitsSinceMyReview: Map<number, number>;
  behind: Map<number, number>;
};

export type DeskState = {
  viewer: string;
  home: string;
  repositories: Repository[];
  /** By GitHub slug. */
  data: Map<string, RepoData>;
  /** By `<repoDir>#<number>`. */
  checkouts: Map<string, PrCheckout>;
  /** By thread id. */
  links: Map<string, PrLink>;
  /** By `<slug>#<number>`, as the profile database keeps them. */
  pendingReviews: Map<string, PendingReview>;
  viewedMarks: Map<string, Map<string, string>>;
  drafts: Map<string, ReviewDraft>;
  agentReviews: AgentReview[];
  findings: Finding[];
  /** By slug. */
  rules: Map<string, ReviewRule[]>;
  account: GitHubAccount;
  githubRepositories: GitHubRepo[];
  requests: ReviewRequests[];
  knownClones: KnownClone[];
  folders: Map<string, Omit<FolderFacts, "prSource">>;
  prsOnly: string[];
  /** By repository folder. */
  settings: Map<string, RepositorySettings>;
  accountPins: Map<string, string>;
  /** Folders cloned this session, with the slug each clones. */
  clones: Map<string, string>;
  /** For ids the fakes make up. */
  next: number;
};

let state: DeskState;

/** The live desk, for the area fakes beside this file. */
export function desk(): DeskState {
  return state;
}

/** Replace the desk with exactly `seed`: nothing of the demo stays. */
export function seedPreviewDesk(seed: DeskSeed = {}): void {
  state = build(structuredClone(seed));
}

/** Back to the demo the browser preview opens on. */
export function resetPreviewDesk(): void {
  seedPreviewDesk(demoDesk());
}

const SYNCED_AT = Date.parse("2026-11-05T10:00:00Z");

function build(seed: DeskSeed): DeskState {
  const viewer = seed.viewer ?? "ciaran";
  const repositories = (seed.repositories ?? []).map((entry) => entry.repository);
  const data = new Map<string, RepoData>();
  const checkouts = new Map<string, PrCheckout>();
  const settings = new Map<string, RepositorySettings>();
  for (const entry of seed.repositories ?? []) {
    const { repository } = entry;
    const dir = repository.path ?? repository.slug;
    for (const checkout of entry.checkouts ?? []) checkouts.set(`${dir}#${checkout.number}`, checkout);
    if (entry.settings) settings.set(dir, { ...defaultSettings(dir, "origin"), ...entry.settings, repoKey: dir });
    if (repository.slug) data.set(repository.slug, repoData(entry, viewer));
  }
  return {
    viewer,
    home: seed.home ?? "/Users/ciaran/.codex",
    repositories,
    data,
    checkouts,
    links: new Map((seed.links ?? []).map((link) => [link.threadId, link])),
    pendingReviews: new Map(),
    viewedMarks: new Map(),
    drafts: new Map(),
    agentReviews: seed.agentReviews ?? [],
    findings: seed.findings ?? [],
    rules: new Map(),
    account: seed.github?.account ?? { login: viewer, tokenSource: "keyring", logins: [viewer] },
    githubRepositories: seed.github?.repositories ?? [],
    requests: seed.github?.requests ?? [],
    knownClones: seed.github?.knownClones ?? [],
    folders: new Map((seed.folders ?? []).map((folder) => [folder.root, folder])),
    prsOnly: repositories.filter((repo) => repo.state === "prsOnly").map((repo) => repo.slug),
    settings,
    accountPins: new Map(),
    clones: new Map(),
    next: 0,
  };
}

function repoData(entry: RepositorySeed, viewer: string): RepoData {
  const prs = entry.prs ?? [];
  const byNumber = new Map(prs.map((pr) => [pr.number, pr]));
  const seeds = entry.stacks ?? [];
  const stacks: StackListing = {
    supported: seeds.length > 0,
    stacks: seeds.map((stack) => ({
      number: stack.number,
      baseRef: stack.baseRef ?? "main",
      open: true,
      prs: stack.prs.map((number) => {
        const pr = byNumber.get(number);
        const merged = stack.merged?.includes(number) ?? false;
        return {
          number,
          state: merged ? "closed" : "open",
          isDraft: pr?.isDraft ?? false,
          merged,
          headRef: pr?.headRef ?? "",
          headSha: pr?.headSha ?? "",
        };
      }),
    })),
  };
  const sims = new Map(
    seeds.map((stack) => {
      const behind = stack.behind ?? 0;
      const sim: StackSim = {
        behind,
        drifted: stack.drifted ?? [],
        worktree: stack.worktree ?? null,
        conflict: stack.conflict ?? null,
        commits: new Map(Object.entries(stack.commits ?? {}).map(([number, count]) => [Number(number), count])),
        baseCommits:
          stack.baseCommits ??
          Array.from({ length: behind }, (_, index) => ({
            sha: `base${index}`.padEnd(40, "0"),
            subject: `Commit ${behind - index} on ${stack.baseRef ?? "main"}`,
          })),
        run: null,
      };
      return [stack.number, sim];
    }),
  );
  const toMap = (record: Record<number, number> = {}) =>
    new Map(Object.entries(record).map(([number, count]) => [Number(number), count]));
  const feed = entry.feed ?? [];
  return {
    inbox: {
      repo: entry.repository.slug,
      viewer,
      prs,
      people: entry.people ?? [],
      events: entry.events ?? [],
      rateLimit: { cost: 1, remaining: 4990, resetAt: "2026-11-05T11:00:00Z" },
      complete: true,
      syncedAt: entry.syncedAt ?? SYNCED_AT,
      triage: [],
      counts: { needsMe: 0, mine: 0, reviewRequested: 0, everyone: 0 },
      feed: [],
    },
    stacks,
    sims,
    details: new Map((entry.details ?? []).map((detail) => [detail.summary.number, detail])),
    feed: feed.map((item, index) => ({ ...item, id: String(feed.length - index) })),
    muted: new Map(),
    commitsSinceMyReview: toMap(entry.commitsSinceMyReview),
    behind: toMap(entry.behind),
  };
}

/** A repository's settings before anyone saved any. */
export function defaultSettings(repoKey: string, prSource: RepositorySettings["prSource"]): RepositorySettings {
  return {
    repoKey,
    setupCommand: null,
    reviewCheckoutPolicy: "ask",
    skipCheckoutConfirm: false,
    worktreeDir: null,
    prSource,
    updatedAt: 0,
  };
}

// --- lookups ----------------------------------------------------------------------

/** The desk repository a review command's `repoDir` names: its folder, or
 *  its slug for a PRs-only repository. Unknown folders are the first one, so
 *  screens that pass a made-up folder still find a repository. */
export function deskRepo(repoDir: string): Repository | undefined {
  return (
    state.repositories.find((repo) => repo.path === repoDir || (repo.slug && repo.slug === repoDir)) ??
    state.repositories[0]
  );
}

/** The live data of the repository at `repoDir`, for the fakes to edit. */
export function liveRepo(repoDir: string): RepoData {
  const data = state.data.get(deskRepo(repoDir)?.slug ?? "");
  if (!data) throw new Error("This folder has no GitHub repository (no matching remote)");
  return data;
}

export function deskInbox(repoDir: string): Inbox {
  return withActivity(liveRepo(repoDir));
}

export function deskStacks(repoDir: string): StackListing {
  return structuredClone(liveRepo(repoDir).stacks);
}

/** A PR's detail: its own when the seed has one, else one made from its row. */
export function deskPrDetail(repoDir: string, number: number): PrDetail {
  const detail = structuredClone(liveDetail(repoDir, number));
  const large = largeDiffShape();
  if (large) detail.files = largeDiffFiles(large.files, large.lines);
  return detail;
}

/** The detail the fakes edit: a reply or a review lands here. */
function liveDetail(repoDir: string, number: number): PrDetail {
  const data = liveRepo(repoDir);
  let detail = data.details.get(number);
  if (!detail) {
    const pr = data.inbox.prs.find((candidate) => candidate.number === number);
    if (!pr) throw new Error(`#${number} is not an open pull request`);
    detail = prDetail(pr);
    data.details.set(number, detail);
  }
  return detail;
}

/** `?largeDiff=<files>x<lines>` in the preview's URL replaces every PR's
 *  files with a generated large diff: the diff scroll benchmark opens
 *  `?largeDiff=40x150`, and it reproduces a slow diff by hand. */
function largeDiffShape(): { files: number; lines: number } | null {
  if (typeof location === "undefined") return null;
  const match = /^(\d+)x(\d+)$/.exec(new URLSearchParams(location.search).get("largeDiff") ?? "");
  return match ? { files: Number(match[1]), lines: Number(match[2]) } : null;
}

// --- the inbox's triage and seen state ---------------------------------------

/** The desk service's Needs me rules (`review/desk/sync.rs`), for the fakes. */
function needsMe(pr: InboxPr, viewer: string): NeedsMe | null {
  if (pr.author !== viewer) return pr.reviewRequests.includes(viewer) ? "reviewRequested" : null;
  if (pr.reviewDecision === "CHANGES_REQUESTED") return "changesRequested";
  if (pr.checks === "FAILURE" || pr.checks === "ERROR") return "checksFailing";
  if (pr.unresolvedThreads > 0) return "unresolved";
  if (!pr.isDraft && pr.reviewDecision === "APPROVED" && (pr.checks === null || pr.checks === "SUCCESS"))
    return "readyToMerge";
  return null;
}

function withActivity(data: RepoData): Inbox {
  const base = structuredClone(data.inbox);
  const triage: PrTriage[] = base.prs.map((pr) => {
    const mine = pr.author === base.viewer;
    const myReview = pr.reviews.find((review) => review.author === base.viewer) ?? null;
    const commitsSinceMyReview = myReview?.commit ? (data.commitsSinceMyReview.get(pr.number) ?? 0) : null;
    const isMuted = data.muted.get(pr.number) === pr.headSha;
    return {
      number: pr.number,
      mine,
      reviewRequested: !mine && (pr.reviewRequests.includes(base.viewer) || (commitsSinceMyReview ?? 0) > 0),
      needsMe: isMuted ? null : needsMe(pr, base.viewer),
      myReview: myReview?.state ?? null,
      commitsSinceMyReview,
      newActivity: data.feed.filter((entry) => entry.number === pr.number).length,
      muted: isMuted,
    };
  });
  const count = (keep: (entry: PrTriage) => boolean) => triage.filter(keep).length;
  return {
    ...base,
    triage,
    counts: {
      needsMe: count((entry) => entry.needsMe !== null),
      mine: count((entry) => entry.mine),
      reviewRequested: count((entry) => entry.reviewRequested),
      everyone: triage.length,
    },
    feed: structuredClone(data.feed),
  };
}

// --- reviews and comments --------------------------------------------------------

const prKey = (repoDir: string, number: number) => `${deskRepo(repoDir)?.slug}#${number}`;
const now = () => new Date().toISOString().replace(/\.\d+Z$/, "Z");
const STATES: Record<string, string> = {
  APPROVE: "APPROVED",
  REQUEST_CHANGES: "CHANGES_REQUESTED",
  approve: "APPROVED",
  requestChanges: "CHANGES_REQUESTED",
};

function comment(fields: Partial<PrComment> & Pick<PrComment, "body">): PrComment {
  state.next += 1;
  return {
    id: 90_000 + state.next,
    author: state.viewer,
    createdAt: now(),
    path: null,
    line: null,
    side: null,
    threadId: null,
    isResolved: false,
    reviewId: null,
    ...NO_ANCHOR,
    ...fields,
  };
}

/** Recount the inbox row's unresolved threads from the detail. */
function recountThreads(repoDir: string, number: number) {
  const detail = liveDetail(repoDir, number);
  const pr = liveRepo(repoDir).inbox.prs.find((candidate) => candidate.number === number);
  if (!pr) return;
  pr.unresolvedThreads = new Set(
    detail.comments.filter((entry) => entry.threadId && !entry.isResolved).map((entry) => entry.threadId),
  ).size;
}

/** Post one review on PR `number`, as GitHub records it: the review, its new
 *  inline comments as threads of their own, and its replies in their threads. */
function postReview(
  repoDir: string,
  number: number,
  review: {
    event: string;
    body: string;
    comments: PendingComment[];
    replies?: { commentId: number; body: string }[];
  },
) {
  const detail = liveDetail(repoDir, number);
  const id = `PRR_preview_${++state.next}`;
  const reviewState = STATES[review.event] ?? "COMMENTED";
  detail.reviews.push({ id, author: state.viewer, state: reviewState, body: review.body, submittedAt: now() });
  for (const posted of review.comments) {
    detail.comments.push(
      comment({
        body: posted.body,
        path: posted.path,
        line: posted.line,
        startLine: posted.startLine ?? null,
        side: posted.side ?? "RIGHT",
        threadId: `PRRT_preview_${state.next + 1}`,
        reviewId: id,
      }),
    );
  }
  for (const reply of review.replies ?? []) replyIn(detail, reply.commentId, reply.body, id);
  const pr = liveRepo(repoDir).inbox.prs.find((candidate) => candidate.number === number);
  if (pr) {
    pr.reviews = [
      ...pr.reviews.filter((entry) => entry.author !== state.viewer),
      { author: state.viewer, state: reviewState, commit: detail.headSha },
    ];
  }
  recountThreads(repoDir, number);
}

function replyIn(detail: PrDetail, commentId: number, body: string, reviewId: string | null) {
  const parent = detail.comments.find((entry) => entry.id === commentId);
  if (!parent?.threadId) throw new Error("That comment is gone");
  detail.comments.push(
    comment({
      body,
      path: parent.path,
      line: parent.line,
      side: parent.side,
      threadId: parent.threadId,
      isResolved: parent.isResolved,
      reviewId: reviewId ?? `PRR_preview_reply_${state.next + 1}`,
    }),
  );
}

/** Each pending entry as the comment it posts. */
function submission(review: PendingReview) {
  const comments: PendingComment[] = [];
  const replies: { commentId: number; body: string }[] = [];
  for (const entry of review.entries) {
    if (entry.kind === "reply") {
      replies.push({ commentId: entry.commentId, body: entry.body });
      continue;
    }
    const body =
      entry.kind === "suggestion" ? `${entry.body}\n\n\`\`\`suggestion\n${entry.replacement}\n\`\`\`` : entry.body;
    comments.push({ ...entry.anchor, body });
  }
  return { event: review.verdict, body: review.summary, comments, replies };
}

function findComment(repoDir: string, ref: CommentRef): { detail: PrDetail; index: number } {
  for (const detail of liveRepo(repoDir).details.values()) {
    const index = detail.comments.findIndex((entry) => entry.id === ref.id && (ref.kind === "review") === !!entry.path);
    if (index >= 0) return { detail, index };
  }
  throw new Error("That comment is gone");
}

/** The inbox, activity, review and comment fakes. */
export const deskCommands = {
  reviewListPrs: (repoDir) => liveRepo(repoDir).inbox.prs.map(prSummary),
  reviewInbox: (repoDir) => deskInbox(repoDir),
  reviewStacks: (repoDir) => deskStacks(repoDir),
  reviewPrDetail: (repoDir, number) => deskPrDetail(repoDir, number),
  reviewFetchPrs: (repoDir, numbers) => {
    const prs = liveRepo(repoDir).inbox.prs;
    return numbers.map((number) => ({ number, headSha: prs.find((pr) => pr.number === number)?.headSha ?? null }));
  },
  reviewMarkActivitySeen(repoDir, numbers) {
    const data = liveRepo(repoDir);
    data.feed = data.feed.filter((entry) => !numbers.includes(entry.number));
    return null;
  },
  reviewMutePr(repoDir, number, headSha) {
    liveRepo(repoDir).muted.set(number, headSha);
    return null;
  },

  reviewPendingReview(repoDir, number) {
    const review = state.pendingReviews.get(prKey(repoDir, number));
    return review ? structuredClone(review) : null;
  },
  reviewSavePendingReview(repoDir, number, review) {
    const key = prKey(repoDir, number);
    if (review.entries.length === 0 && !review.summary.trim() && review.verdict !== "approve")
      state.pendingReviews.delete(key);
    else state.pendingReviews.set(key, { ...structuredClone(review), updatedAt: Math.floor(Date.now() / 1000) });
    return null;
  },
  reviewDiscardPendingReview(repoDir, number) {
    state.pendingReviews.delete(prKey(repoDir, number));
    return null;
  },
  reviewSubmitPendingReview(repoDir, number) {
    const key = prKey(repoDir, number);
    const review = state.pendingReviews.get(key);
    if (!review) throw new Error("There is no pending review to submit");
    postReview(repoDir, number, submission(review));
    state.pendingReviews.delete(key);
    return null;
  },
  reviewSubmit(repoDir, number, event, body, _commitId, comments) {
    postReview(repoDir, number, { event, body, comments });
    return null;
  },
  reviewReply(repoDir, number, commentId, body) {
    replyIn(liveDetail(repoDir, number), commentId, body, null);
    return null;
  },
  reviewResolveThread(repoDir, threadId) {
    const data = liveRepo(repoDir);
    for (const [number, detail] of data.details) {
      const thread = detail.comments.filter((entry) => entry.threadId === threadId);
      if (thread.length === 0) continue;
      for (const entry of thread) entry.isResolved = true;
      recountThreads(repoDir, number);
      return null;
    }
    throw new Error("That review thread is gone");
  },
  reviewEditComment(repoDir, ref, body) {
    const { detail, index } = findComment(repoDir, ref);
    detail.comments[index] = { ...detail.comments[index], body };
    return null;
  },
  reviewDeleteComment(repoDir, ref) {
    const { detail, index } = findComment(repoDir, ref);
    detail.comments.splice(index, 1);
    return null;
  },

  reviewViewedFiles(repoDir, number, files: FileBlob[]) {
    const marks = state.viewedMarks.get(prKey(repoDir, number));
    return files.filter((file) => marks?.get(file.path) === file.blobSha).map((file) => file.path);
  },
  reviewSetViewed(repoDir, number, files, viewed) {
    const key = prKey(repoDir, number);
    const marks = state.viewedMarks.get(key) ?? new Map<string, string>();
    for (const file of files) {
      if (viewed) marks.set(file.path, file.blobSha);
      else marks.delete(file.path);
    }
    state.viewedMarks.set(key, marks);
    return null;
  },
  reviewSaveDraft(provider, repo, prNumber, headSha, payload) {
    state.drafts.set(`${provider}:${repo}:${prNumber}`, {
      headSha,
      payload,
      updatedAt: Math.floor(Date.now() / 1000),
    });
    return null;
  },
  reviewLoadDraft: (provider, repo, prNumber) => state.drafts.get(`${provider}:${repo}:${prNumber}`) ?? null,
  reviewDeleteDraft(provider, repo, prNumber) {
    state.drafts.delete(`${provider}:${repo}:${prNumber}`);
    return null;
  },
} satisfies Partial<DeskCommands>;

// The demo until a test seeds its own. The vitest setup resets the desk before
// each test, once a test has loaded it.
resetPreviewDesk();
(globalThis as { __resetPreviewDesk?: () => void }).__resetPreviewDesk = resetPreviewDesk;
