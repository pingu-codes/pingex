// The review desk's base data for the browser preview and the desk UI tests:
// several repositories (one PRs only, one with no GitHub remote), a
// three-PR stack of mine, a stack another author asks me to review, a PR
// with human review threads, and agent reviews queued and running.
//
// The review command fakes in `commands.ts` answer from here, by the
// repository's folder, or by its slug for a PRs-only repository. Viewer is
// `ciaran` throughout. Extend these records rather than adding parallel ones,
// so every desk screen shows the same repositories and PRs.

import type { CheckoutStep, HarnessKind, Inbox, InboxPr, PrComment, PrDetail, StackListing } from "$lib/types";
import { inboxPr } from "./factories";
import { previewInbox, previewPrDetail } from "./fixtures";

export const DESK_VIEWER = "ciaran";

/** A repository as the desk shows it. Stands in until Add repository gives
 *  the backend a repository record. */
export type PreviewRepository = {
  /** `owner/name`; empty with no GitHub remote. */
  slug: string;
  name: string;
  /** The clone's folder; `null` for a PRs-only repository. */
  path: string | null;
  state: "cloned" | "prsOnly" | "noRemote";
};

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

export const DESK_REPOS: PreviewRepository[] = [
  { slug: "ciaran/codex-custom", name: "codex-custom", path: "/Users/ciaran/Projects/codex-custom", state: "cloned" },
  {
    slug: "ciaran/arctic-explorer",
    name: "arctic-explorer",
    path: "/Users/ciaran/Projects/arctic-explorer",
    state: "cloned",
  },
  { slug: "openai/codex", name: "codex", path: null, state: "prsOnly" },
  { slug: "", name: "notes", path: "/Users/ciaran/Projects/notes", state: "noRemote" },
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

function inbox(slug: string, prs: InboxPr[]): Inbox {
  return {
    repo: slug,
    viewer: DESK_VIEWER,
    prs,
    events: [],
    rateLimit: { cost: 1, remaining: 4990, resetAt: "2026-11-05T11:00:00Z" },
    complete: true,
    syncedAt: Date.parse("2026-11-05T10:00:00Z"),
  };
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
export function deskRepo(repoDir: string): PreviewRepository {
  return DESK_REPOS.find((repo) => repo.path === repoDir || (repo.slug && repo.slug === repoDir)) ?? DESK_REPOS[0];
}

function repoData(repoDir: string): RepoData {
  const repo = deskRepo(repoDir);
  const data = DATA.get(repo.slug);
  if (!data) throw new Error("This folder has no GitHub repository (no matching remote)");
  return data;
}

export function deskInbox(repoDir: string): Inbox {
  return structuredClone(repoData(repoDir).inbox);
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
