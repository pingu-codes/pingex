// The review desk's base data for the browser preview and the desk UI tests:
// several repositories (one PRs only, one with no GitHub remote), a
// three-PR stack of mine, a stack another author asks me to review, a PR
// with human review threads, and agent reviews queued and running.
//
// The review command fakes in `commands.ts` answer from here, by the
// repository's folder, or by its slug for a PRs-only repository. Viewer is
// `ciaran` throughout. Extend these records rather than adding parallel ones,
// so every desk screen shows the same repositories and PRs.

import type {
  CheckoutStep,
  FeedEntry,
  FileBlob,
  HarnessKind,
  Inbox,
  InboxPr,
  NeedsMe,
  PendingReview,
  PrComment,
  PrDetail,
  PrTriage,
  Repository,
  StackListing,
} from "$lib/types";
import { inboxPr } from "./factories";
import { previewInbox, previewPrDetail } from "./fixtures";

export const DESK_VIEWER = "ciaran";

/** An agent review in flight. Stands in until agent reviews have a backend
 *  record. */
export type PreviewAgentReview = {
  id: string;
  repo: string;
  number: number;
  harness: HarnessKind;
  /** Where it runs: a diff review at No checkout, or with tests. */
  step: CheckoutStep;
  status: "queued" | "running";
  /** Unix milliseconds; `null` while queued. */
  startedAt: number | null;
};

/** A clone at `path` fed by its `origin`, `slug`. */
function cloned(slug: string, path: string): Repository {
  return {
    key: path,
    name: path.split("/").at(-1) ?? path,
    slug,
    path,
    state: "cloned",
    feeds: [{ remote: "origin", slug, onlyMine: false }],
  };
}

export const DESK_REPOS: Repository[] = [
  cloned("ciaran/codex-custom", "/Users/ciaran/Projects/codex-custom"),
  cloned("ciaran/arctic-explorer", "/Users/ciaran/Projects/arctic-explorer"),
  {
    key: "openai/codex",
    name: "codex",
    slug: "openai/codex",
    path: null,
    state: "prsOnly",
    feeds: [{ remote: "", slug: "openai/codex", onlyMine: false }],
  },
  {
    key: "/Users/ciaran/Projects/notes",
    name: "notes",
    slug: "",
    path: "/Users/ciaran/Projects/notes",
    state: "noRemote",
    feeds: [],
  },
];

const url = (slug: string, number: number) => `https://github.com/${slug}/pull/${number}`;
const sha = (seed: number) => String(seed).repeat(40).slice(0, 40);

// --- ciaran/codex-custom ---------------------------------------------------

/** My stack, bottom first: main ← feat/driver-trait ← feat/claude-driver ← feat/maki-acp. */
export const MY_STACK = [412, 415, 418] as const;
/** jo-r's stack asking me for review: main ← jo-r/focus-rings ← jo-r/focus-trap. */
export const THEIR_STACK = [401, 403] as const;
/** The PR with human review threads. */
export const THREADED_PR = 409;

const mainPrs: InboxPr[] = [
  ...previewInbox.prs,
  inboxPr({
    number: 412,
    title: "Harness driver trait",
    headRef: "feat/driver-trait",
    reviewDecision: "APPROVED",
    reviews: [{ author: "mira-k", state: "APPROVED" }],
  }),
  inboxPr({
    number: 415,
    title: "Claude Code driver over stream-json",
    headRef: "feat/claude-driver",
    baseRef: "feat/driver-trait",
    baseSha: sha(412),
    reviewDecision: "CHANGES_REQUESTED",
    reviews: [{ author: "mira-k", state: "CHANGES_REQUESTED" }],
    unresolvedThreads: 2,
  }),
  inboxPr({
    number: 418,
    title: "maki over ACP",
    isDraft: true,
    headRef: "feat/maki-acp",
    baseRef: "feat/claude-driver",
    baseSha: sha(415),
    checks: "FAILURE",
  }),
  inboxPr({
    number: 401,
    title: "Focus rings on every control",
    author: "jo-r",
    headRef: "jo-r/focus-rings",
    headRepo: "jo-r/codex-custom",
    maintainerCanModify: true,
    reviewDecision: "REVIEW_REQUIRED",
    reviewRequests: [DESK_VIEWER],
    // Approved, then two more commits and a re-request.
    reviews: [{ author: DESK_VIEWER, state: "APPROVED", commit: sha(400) }],
  }),
  inboxPr({
    number: 403,
    title: "Trap focus in dialogs",
    author: "jo-r",
    headRef: "jo-r/focus-trap",
    baseRef: "jo-r/focus-rings",
    baseSha: sha(401),
    headRepo: "jo-r/codex-custom",
    reviewDecision: "REVIEW_REQUIRED",
    reviewRequests: [DESK_VIEWER],
    checks: "PENDING",
  }),
  inboxPr({
    number: THREADED_PR,
    title: "Retry the app-server handshake",
    author: "mira-k",
    headRef: "mira-k/handshake-retry",
    reviewDecision: "CHANGES_REQUESTED",
    reviewRequests: [DESK_VIEWER],
    reviews: [{ author: "tomasz", state: "CHANGES_REQUESTED" }],
    unresolvedThreads: 2,
    additions: 86,
    deletions: 12,
    changedFiles: 2,
  }),
  // Someone else's, not asking me: only under Everyone.
  inboxPr({
    number: 420,
    title: "Bump vitest to 4.1",
    author: "tomasz",
    headRef: "tomasz/vitest-4.1",
    reviewDecision: "REVIEW_REQUIRED",
    reviewRequests: ["mira-k"],
  }),
];

const mainStacks: StackListing = {
  supported: true,
  stacks: [
    { number: 7, baseRef: "main", open: true, prs: [] },
    { number: 8, baseRef: "main", open: true, prs: [] },
  ],
};
for (const [index, numbers] of [MY_STACK, THEIR_STACK].entries()) {
  mainStacks.stacks[index].prs = numbers.map((number) => {
    const pr = mainPrs.find((candidate) => candidate.number === number) as InboxPr;
    return {
      number,
      state: "open",
      isDraft: pr.isDraft,
      merged: false,
      headRef: pr.headRef,
      headSha: pr.headSha,
    };
  });
}

function thread(
  threadId: string,
  path: string,
  line: number,
  resolved: boolean,
  messages: [author: string, body: string][],
): PrComment[] {
  return messages.map(([author, body], index) => ({
    id: Number.parseInt(threadId.replace(/\D/g, ""), 10) * 10 + index,
    author,
    body,
    createdAt: `2026-11-04T1${index}:00:00Z`,
    path,
    line,
    side: "RIGHT",
    threadId,
    isResolved: resolved,
  }));
}

const threadedDetail: PrDetail = {
  ...structuredClone(previewPrDetail),
  summary: {
    number: THREADED_PR,
    title: "Retry the app-server handshake",
    author: "mira-k",
    state: "OPEN",
    isDraft: false,
    baseRef: "main",
    headRef: "mira-k/handshake-retry",
    updatedAt: "2026-11-05T09:00:00Z",
    url: url("ciaran/codex-custom", THREADED_PR),
  },
  body: "The app-server sometimes answers `initialize` late on a cold start. Retry the handshake three times with backoff.",
  headSha: sha(THREADED_PR),
  baseSha: sha(1),
  comments: [
    ...thread("THREAD_1", "src/lib/loader.ts", 4, false, [
      ["tomasz", "Three retries feels arbitrary. Can this come from the config?"],
      ["mira-k", "It could, but nobody has asked to tune it yet."],
    ]),
    ...thread("THREAD_2", "src/lib/loader.ts", 6, false, [["tomasz", "This swallows the original error."]]),
    ...thread("THREAD_3", "docs/review.md", 1, true, [
      ["ciaran", "Typo in the heading."],
      ["mira-k", "Fixed."],
    ]),
    {
      id: 0,
      author: "tomasz",
      body: "Requesting changes for the error handling; the rest looks good.",
      createdAt: "2026-11-04T15:00:00Z",
      path: null,
      line: null,
      side: null,
      threadId: null,
      isResolved: false,
    },
  ],
};

// --- the other repositories --------------------------------------------------

const arcticPrs: InboxPr[] = [
  inboxPr({
    number: 31,
    title: "Tile cache for the map view",
    url: url("ciaran/arctic-explorer", 31),
    headRef: "feat/tile-cache",
    checks: "FAILURE",
  }),
];

const codexPrs: InboxPr[] = [
  inboxPr({
    number: 7120,
    title: "app-server: report rate limits per model",
    author: "openai-dev",
    url: url("openai/codex", 7120),
    headRef: "rate-limits-per-model",
    reviewDecision: "REVIEW_REQUIRED",
    reviewRequests: [DESK_VIEWER],
  }),
  inboxPr({
    number: 7104,
    title: "tui: wrap long tool names",
    author: "contributor-9",
    url: url("openai/codex", 7104),
    headRef: "wrap-tool-names",
    headRepo: "contributor-9/codex",
  }),
];

// --- lookups the command fakes use -----------------------------------------

type RepoData = { inbox: Inbox; stacks: StackListing; details: Map<number, PrDetail> };

const SYNCED_AT = Date.parse("2026-11-05T10:00:00Z");
const minutesAgo = (minutes: number) => SYNCED_AT - minutes * 60_000;

function inbox(slug: string, prs: InboxPr[]): Inbox {
  return {
    repo: slug,
    viewer: DESK_VIEWER,
    prs,
    events: [],
    rateLimit: { cost: 1, remaining: 4990, resetAt: "2026-11-05T11:00:00Z" },
    complete: true,
    syncedAt: SYNCED_AT,
    triage: [],
    counts: { needsMe: 0, mine: 0, reviewRequested: 0, everyone: 0 },
    feed: [],
  };
}

/** Commits since my latest review, which the backend counts in the clone. */
const COMMITS_SINCE_MY_REVIEW = new Map([[401, 2]]);

/** "Since you last looked" for the main repository, newest first. */
const MAIN_FEED: Omit<FeedEntry, "id">[] = [
  { number: 415, kind: "reviewed", actor: "mira-k", state: "CHANGES_REQUESTED", at: minutesAgo(12) },
  { number: 418, kind: "checksFailed", actor: null, state: null, at: minutesAgo(60) },
  { number: 401, kind: "pushed", actor: "jo-r", state: null, at: minutesAgo(120) },
  { number: 401, kind: "reviewRequested", actor: null, state: null, at: minutesAgo(120) },
  { number: 412, kind: "reviewed", actor: "mira-k", state: "APPROVED", at: minutesAgo(180) },
];

// The seen state the activity fakes keep, per repository slug.
const feeds = new Map<string, FeedEntry[]>();
const muted = new Map<string, Map<number, string>>();

/** Back to the seeded feed and no mutes. Tests call it between cases. */
export function resetDeskActivity() {
  feeds.clear();
  muted.clear();
  feeds.set(
    "ciaran/codex-custom",
    MAIN_FEED.map((entry, index) => ({ ...entry, id: String(MAIN_FEED.length - index) })),
  );
}
resetDeskActivity();

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

function withActivity(base: Inbox): Inbox {
  const feed = feeds.get(base.repo) ?? [];
  const mutes = muted.get(base.repo) ?? new Map<number, string>();
  const triage: PrTriage[] = base.prs.map((pr) => {
    const mine = pr.author === base.viewer;
    const myReview = pr.reviews.find((review) => review.author === base.viewer) ?? null;
    const commitsSinceMyReview = myReview?.commit ? (COMMITS_SINCE_MY_REVIEW.get(pr.number) ?? 0) : null;
    const isMuted = mutes.get(pr.number) === pr.headSha;
    return {
      number: pr.number,
      mine,
      reviewRequested: !mine && (pr.reviewRequests.includes(base.viewer) || (commitsSinceMyReview ?? 0) > 0),
      needsMe: isMuted ? null : needsMe(pr, base.viewer),
      myReview: myReview?.state ?? null,
      commitsSinceMyReview,
      newActivity: feed.filter((entry) => entry.number === pr.number).length,
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
    feed: structuredClone(feed),
  };
}

export function deskMarkActivitySeen(repoDir: string, numbers: number[]) {
  const slug = repoData(repoDir).inbox.repo;
  feeds.set(
    slug,
    (feeds.get(slug) ?? []).filter((entry) => !numbers.includes(entry.number)),
  );
}

export function deskMutePr(repoDir: string, number: number, headSha: string) {
  const slug = repoData(repoDir).inbox.repo;
  const mutes = muted.get(slug) ?? new Map<number, string>();
  mutes.set(number, headSha);
  muted.set(slug, mutes);
}

const unsupported: StackListing = { supported: false, stacks: [] };

const DATA = new Map<string, RepoData>([
  [
    "ciaran/codex-custom",
    {
      inbox: { ...inbox("ciaran/codex-custom", mainPrs), events: previewInbox.events },
      stacks: mainStacks,
      details: new Map([[THREADED_PR, threadedDetail]]),
    },
  ],
  [
    "ciaran/arctic-explorer",
    { inbox: inbox("ciaran/arctic-explorer", arcticPrs), stacks: unsupported, details: new Map() },
  ],
  ["openai/codex", { inbox: inbox("openai/codex", codexPrs), stacks: unsupported, details: new Map() }],
]);

/** The desk repository a review command's `repoDir` names: its folder, or
 *  its slug for a PRs-only repository. Unknown folders are the main one, so
 *  older preview screens keep working. */
export function deskRepo(repoDir: string): Repository {
  return DESK_REPOS.find((repo) => repo.path === repoDir || (repo.slug && repo.slug === repoDir)) ?? DESK_REPOS[0];
}

function repoData(repoDir: string): RepoData {
  const repo = deskRepo(repoDir);
  const data = DATA.get(repo.slug);
  if (!data) throw new Error("This folder has no GitHub repository (no matching remote)");
  return data;
}

export function deskInbox(repoDir: string): Inbox {
  return withActivity(structuredClone(repoData(repoDir).inbox));
}

export function deskStacks(repoDir: string): StackListing {
  return structuredClone(repoData(repoDir).stacks);
}

/** A PR's detail: its own when the fixtures have one, else the shared
 *  sample detail under the PR's summary. */
export function deskPrDetail(repoDir: string, number: number): PrDetail {
  const data = repoData(repoDir);
  const own = data.details.get(number);
  if (own) return structuredClone(own);
  const detail = structuredClone(previewPrDetail);
  const pr = data.inbox.prs.find((candidate) => candidate.number === number);
  if (pr) {
    detail.summary = {
      number: pr.number,
      title: pr.title,
      author: pr.author,
      state: "OPEN",
      isDraft: pr.isDraft,
      baseRef: pr.baseRef,
      headRef: pr.headRef,
      updatedAt: pr.updatedAt,
      url: pr.url,
    };
    detail.headSha = pr.headSha;
    detail.baseSha = pr.baseSha;
  }
  return detail;
}

export const previewAgentReviews: PreviewAgentReview[] = [
  {
    id: "agent-review-1",
    repo: "ciaran/codex-custom",
    number: THREADED_PR,
    harness: "claude",
    step: "noCheckout",
    status: "running",
    startedAt: Date.parse("2026-11-05T09:58:00Z"),
  },
  {
    id: "agent-review-2",
    repo: "ciaran/codex-custom",
    number: 403,
    harness: "codex",
    step: "reviewCheckout",
    status: "queued",
    startedAt: null,
  },
];

// --- the pending review and viewed files ------------------------------------

/** Pending reviews and viewed marks, per repository and PR, as the profile
 *  database keeps them: they outlive a remount of the review page. */
const pendingReviews = new Map<string, PendingReview>();
const viewedMarks = new Map<string, Map<string, string>>();
const prKey = (repoDir: string, number: number) => `${deskRepo(repoDir).slug}#${number}`;

export function deskPendingReview(repoDir: string, number: number): PendingReview | null {
  const review = pendingReviews.get(prKey(repoDir, number));
  return review ? structuredClone(review) : null;
}

export function deskSavePendingReview(repoDir: string, number: number, review: PendingReview) {
  const key = prKey(repoDir, number);
  if (review.entries.length === 0 && !review.summary.trim()) pendingReviews.delete(key);
  else pendingReviews.set(key, { ...structuredClone(review), updatedAt: Math.floor(Date.now() / 1000) });
}

export function deskDiscardPendingReview(repoDir: string, number: number) {
  pendingReviews.delete(prKey(repoDir, number));
}

export function deskSubmitPendingReview(repoDir: string, number: number) {
  const key = prKey(repoDir, number);
  if (!pendingReviews.has(key)) throw new Error("There is no pending review to submit");
  pendingReviews.delete(key);
}

export function deskViewedFiles(repoDir: string, number: number, files: FileBlob[]): string[] {
  const marks = viewedMarks.get(prKey(repoDir, number));
  return files.filter((file) => marks?.get(file.path) === file.blobSha).map((file) => file.path);
}

export function deskSetViewed(repoDir: string, number: number, files: FileBlob[], viewed: boolean) {
  const key = prKey(repoDir, number);
  const marks = viewedMarks.get(key) ?? new Map<string, string>();
  for (const file of files) {
    if (viewed) marks.set(file.path, file.blobSha);
    else marks.delete(file.path);
  }
  viewedMarks.set(key, marks);
}

/** Forget every pending review and viewed mark: a fresh profile, for tests. */
export function resetDeskReviewState() {
  pendingReviews.clear();
  viewedMarks.clear();
}
