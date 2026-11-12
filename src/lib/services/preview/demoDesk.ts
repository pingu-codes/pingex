// The review desk the browser preview opens on, and the Playwright suite
// drives: several repositories (one PRs only, one with no GitHub remote), a
// three-PR stack of mine, a stack another author asks me to review, a PR
// with human review threads, agent reviews queued, running and finished, and
// the account and folders Add repository finds. Viewer is `ciaran`
// throughout.
//
// This is demo content: extend it so every desk screen of the preview shows
// the same repositories and PRs, but don't test against it. A test seeds the
// desk with what it needs (`seedPreviewDesk` and `./factories`).

import type { AgentReview, FolderFacts, GitHubPerson, GitHubRepo, InboxPr, PrDetail, Repository } from "$lib/types";
import type { DeskSeed, RepositorySeed } from "./desk";
import { agentReview, clonedRepository, inboxPr, prDetail, reviewThread, sha, withAnchors } from "./factories";
import { previewInbox, previewPrDetail } from "./fixtures";

export const DESK_VIEWER = "ciaran";
const HOME = "/Users/ciaran/.codex";
const MAIN = "/Users/ciaran/Projects/codex-custom";

export const DESK_REPOS: Repository[] = [
  clonedRepository("ciaran/codex-custom", MAIN),
  clonedRepository("ciaran/arctic-explorer", "/Users/ciaran/Projects/arctic-explorer"),
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

// --- ciaran/codex-custom ---------------------------------------------------

/** My stack, bottom first: main ← feat/driver-trait ← feat/claude-driver ← feat/maki-acp. */
export const MY_STACK = [412, 415, 418] as const;
/** jo-r's stack asking me for review: main ← jo-r/focus-rings ← jo-r/focus-trap. */
export const THEIR_STACK = [401, 403] as const;
/** The PR with human review threads. */
export const THREADED_PR = 409;

const mainPrs = (): InboxPr[] => [
  ...structuredClone(previewInbox.prs),
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
    assignees: ["mira-k"],
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
    // Assigned to me while 401 below it is not: the stack shows dimmed.
    assignees: [DESK_VIEWER],
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
    assignees: ["tomasz", "mira-k"],
    reviewRequests: ["mira-k"],
  }),
];

/** Every PR's detail is the sample one under the PR's own summary. */
function sampleDetail(pr: InboxPr): PrDetail {
  const { summary: _summary, headSha: _head, baseSha: _base, ...sample } = previewPrDetail;
  const detail = prDetail(pr, sample);
  detail.summary = { ...detail.summary, headRepo: "ciaran/codex-custom", maintainerCanModify: false };
  return detail;
}

function threadedDetail(pr: InboxPr): PrDetail {
  return withAnchors({
    ...sampleDetail(pr),
    body: "The app-server sometimes answers `initialize` late on a cold start. Retry the handshake three times with backoff.",
    comments: [
      ...reviewThread(
        "THREAD_1",
        "src/lib/loader.ts",
        4,
        [
          ["tomasz", "Three retries feels **arbitrary**. Can this come from the _config_?"],
          ["mira-k", "It could, but nobody has asked to tune it yet."],
        ],
        { review: "PRR_tomasz" },
      ),
      ...reviewThread(
        "THREAD_2",
        "src/lib/loader.ts",
        6,
        [
          [
            "tomasz",
            "This swallows the original error. Keep it as the `cause`:\n\n```ts\nthrow new Error('path is required', { cause });\n```",
          ],
        ],
        { review: "PRR_tomasz", anchor: { startLine: 4 } },
      ),
      ...reviewThread(
        "THREAD_3",
        "docs/review.md",
        1,
        [
          ["ciaran", "Typo in the heading."],
          ["mira-k", "Fixed."],
        ],
        { resolved: true, review: "PRR_ciaran", anchor: { outdated: true } },
      ),
      {
        id: 0,
        author: "mira-k",
        body: "Pushed the error-handling fix; the retry count stays fixed for now.",
        createdAt: "2026-11-04T15:00:00Z",
        path: null,
        line: null,
        startLine: null,
        originalLine: null,
        originalStartLine: null,
        diffHunk: null,
        outdated: false,
        side: null,
        threadId: null,
        isResolved: false,
        reviewId: null,
      },
    ],
    reviews: [
      {
        id: "PRR_tomasz",
        author: "tomasz",
        state: "CHANGES_REQUESTED",
        body: "Requesting changes for the error handling; the rest looks good.",
        submittedAt: "2026-11-04T10:00:00Z",
      },
      { id: "PRR_ciaran", author: "ciaran", state: "COMMENTED", body: "", submittedAt: "2026-11-04T10:05:00Z" },
    ],
  });
}

const SYNCED_AT = Date.parse("2026-11-05T10:00:00Z");
const minutesAgo = (minutes: number) => SYNCED_AT - minutes * 60_000;

/** A flat colour for an avatar, so the preview and tests never reach the network. */
const avatar = (color: string) =>
  `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><rect width="1" height="1" fill="${color}"/></svg>`)}`;

const DESK_PEOPLE: GitHubPerson[] = [
  { login: "jo-r", name: "Jo Reyes", avatarUrl: avatar("#0e7490"), isTeam: false },
  { login: "mira-k", name: "Mira Kovac", avatarUrl: avatar("#b45309"), isTeam: false },
  // No display name, and no avatar to fetch: initials.
  { login: "tomasz", name: null, avatarUrl: null, isTeam: false },
];

function mainRepository(): RepositorySeed {
  const prs = mainPrs();
  return {
    repository: DESK_REPOS[0],
    prs,
    people: DESK_PEOPLE,
    events: previewInbox.events,
    details: prs.map((entry) => (entry.number === THREADED_PR ? threadedDetail(entry) : sampleDetail(entry))),
    stacks: [
      // Three behind main, #418 drifted off #415's head, checked out in a
      // worktree, and the first restack stops at a conflict on #415.
      {
        number: 7,
        prs: [...MY_STACK],
        behind: 3,
        drifted: [418],
        worktree: {
          path: "/Users/ciaran/worktrees/codex-custom/harness-stack",
          branch: "feat/claude-driver",
          clean: true,
        },
        conflict: { at: 415, files: ["src-tauri/src/harness/driver.rs"] },
        commits: { 412: 3, 415: 7, 418: 4 },
        baseCommits: [
          { sha: "1a627a2".padEnd(40, "0"), subject: "fix: better thread placements" },
          { sha: "4ef2adb".padEnd(40, "0"), subject: "refactor: move to bun" },
          { sha: "b857f01".padEnd(40, "0"), subject: "feat: harness capabilities" },
        ],
      },
      { number: 8, prs: [...THEIR_STACK], behind: 1, commits: { 401: 3, 403: 7 } },
    ],
    feed: [
      { number: 415, kind: "reviewed", actor: "mira-k", state: "CHANGES_REQUESTED", at: minutesAgo(12) },
      { number: 418, kind: "checksFailed", actor: null, state: null, at: minutesAgo(60) },
      { number: 401, kind: "pushed", actor: "jo-r", state: null, at: minutesAgo(120) },
      { number: 401, kind: "reviewRequested", actor: null, state: null, at: minutesAgo(120) },
      { number: 412, kind: "reviewed", actor: "mira-k", state: "APPROVED", at: minutesAgo(180) },
    ],
    commitsSinceMyReview: { 401: 2 },
    // My single #128 is behind main.
    behind: { 128: 2 },
  };
}

function otherRepository(repository: Repository, prs: InboxPr[]): RepositorySeed {
  return { repository, prs, people: DESK_PEOPLE, details: prs.map(sampleDetail) };
}

// --- Add repository -----------------------------------------------------------

/** A GitHub repository the account owns. */
function mine(name: string, fork: string | null = null): GitHubRepo {
  const slug = `${DESK_VIEWER}/${name}`;
  return {
    slug,
    private: false,
    fork: fork !== null,
    parent: fork,
    defaultBranch: "main",
    cloneUrl: `https://github.com/${slug}.git`,
    sshUrl: `git@github.com:${slug}.git`,
  };
}

/** A fork on disk: `origin` is mine, `upstream` is where its PRs open. */
export const FORK_FOLDER = "/Users/ciaran/Projects/codex-mirror";

const remote = (name: string, slug: string | null) => ({
  name,
  url: slug ? `https://github.com/${slug}.git` : "/srv/git/notes.git",
  slug,
});

const FOLDERS: Omit<FolderFacts, "prSource">[] = [
  {
    root: FORK_FOLDER,
    name: "codex-mirror",
    branch: "main",
    linkedWorktrees: 2,
    remotes: [remote("origin", "ciaran/codex-mirror"), remote("upstream", "openai/codex")],
    lockfile: "bun.lock",
    setupCommand: "bun install",
  },
  {
    root: "/Users/ciaran/Projects/website",
    name: "website",
    branch: "main",
    linkedWorktrees: 0,
    remotes: [remote("origin", "ciaran/website")],
    lockfile: null,
    setupCommand: null,
  },
  {
    root: "/Users/ciaran/Projects/notes",
    name: "notes",
    branch: "main",
    linkedWorktrees: 0,
    remotes: [remote("origin", null)],
    lockfile: null,
    setupCommand: null,
  },
];

// --- the whole desk -------------------------------------------------------------

export function demoDesk(): DeskSeed {
  const main = mainRepository();
  const prs = main.prs ?? [];
  const pr = (number: number) => prs.find((candidate) => candidate.number === number) as InboxPr;
  const run = (id: string, number: number, fields: Partial<AgentReview>) =>
    agentReview(MAIN, "ciaran/codex-custom", pr(number), { createdAt: Date.now(), ...fields, id });
  const done = run("agent-review-1", THREADED_PR, {
    status: "done",
    startedAt: Date.parse("2026-11-05T09:35:00Z"),
    finishedAt: Date.parse("2026-11-05T09:35:00Z"),
    threadId: "preview-agent-409",
    findings: 3,
  });
  const running = run("agent-review-2", 401, {
    status: "running",
    startedAt: Date.parse("2026-11-05T09:58:00Z"),
    threadId: "preview-agent-401",
  });
  const queued = run("agent-review-3", 403, {});
  queued.request = { ...queued.request, harness: "codex" };
  const finding = {
    agentReviewId: done.id,
    prNumber: THREADED_PR,
    headSha: done.headSha,
    side: "RIGHT",
    endLine: null,
    evidence: null,
    sameAsCommentId: null,
    state: "draft" as const,
    dismissReason: null,
    dismissNote: null,
    path: "src/lib/loader.ts",
  };
  return {
    viewer: DESK_VIEWER,
    home: HOME,
    repositories: [
      main,
      otherRepository(DESK_REPOS[1], [
        inboxPr({
          number: 31,
          title: "Tile cache for the map view",
          url: url("ciaran/arctic-explorer", 31),
          headRef: "feat/tile-cache",
          checks: "FAILURE",
        }),
      ]),
      otherRepository(DESK_REPOS[2], [
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
      ]),
      { repository: DESK_REPOS[3] },
    ],
    // Newest first, as the backend lists them.
    agentReviews: [done, running, queued],
    // The finished review's Findings: one on a changed line, one merged into
    // tomasz's comment, one outside the diff.
    findings: [
      {
        ...finding,
        id: `${done.id}-1`,
        line: 7,
        severity: "medium",
        title: "Read errors lose the path",
        body: "readFileSync throws ENOENT without saying which file it was reading, so a missing config reads as a crash.",
        evidence: "loader.ts:7 calls readFileSync(path) with no try/catch.",
      },
      {
        ...finding,
        id: `${done.id}-2`,
        line: 6,
        severity: "low",
        title: "The original error is swallowed",
        body: "The thrown error replaces whatever went wrong first.",
        sameAsCommentId: 20,
        state: "merged",
      },
      {
        ...finding,
        id: `${done.id}-3`,
        line: 1,
        severity: "low",
        title: "Synchronous read on startup",
        body: "readFileSync blocks the event loop while the app starts.",
      },
    ],
    github: {
      account: { login: DESK_VIEWER, tokenSource: "keyring", logins: [DESK_VIEWER, "ciaran-work"] },
      repositories: [
        mine("codex-custom"),
        mine("arctic-explorer"),
        mine("website"),
        mine("dotfiles"),
        mine("codex-mirror", "openai/codex"),
      ],
      // Not added yet: one with no clone anywhere, one with a clone Pingex knows.
      requests: [
        { slug: "tomasz/bun-tools", count: 2 },
        { slug: "ciaran/website", count: 1 },
      ],
      knownClones: [{ slug: "ciaran/website", path: "/Users/ciaran/Projects/website", remote: "origin" }],
    },
    folders: FOLDERS,
  };
}
