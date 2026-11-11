// The repository the inbox tests seed the preview desk with: one of every
// kind of row the inbox sorts. Test data, not the demo: change it together
// with the tests that count on it.
//
// - My stack 7, bottom first: #412 approved, #415 with changes requested
//   and two unresolved threads, #418 a failing draft. Three behind main,
//   #418 drifted off #415's head, checked out in `harness-stack`.
// - jo-r's stack 8 asking me for review: #401 (approved by me two commits
//   ago) and #403 (assigned to me).
// - #128, mine and alone, two behind main; #126, someone's draft asking me.
// - #409 by mira-k asking me, with an agent review done; #420 by tomasz,
//   not asking me.

import { type StackSeed, seedPreviewDesk } from "$lib/services/preview/desk";
import { agentReview, deskRepository, inboxPr, prStack, sha } from "$lib/services/preview/factories";
import type { GitHubPerson } from "$lib/types";

export const SCENARIO_DIR = "/Users/ciaran/Projects/codex-custom";
const SLUG = "ciaran/codex-custom";
const SYNCED_AT = Date.parse("2026-11-05T10:00:00Z");
const minutesAgo = (minutes: number) => SYNCED_AT - minutes * 60_000;

const avatar = (color: string) =>
  `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><rect width="1" height="1" fill="${color}"/></svg>`)}`;

const PEOPLE: GitHubPerson[] = [
  { login: "jo-r", name: "Jo Reyes", avatarUrl: avatar("#0e7490"), isTeam: false },
  { login: "mira-k", name: "Mira Kovac", avatarUrl: avatar("#b45309"), isTeam: false },
  { login: "tomasz", name: null, avatarUrl: null, isTeam: false },
];

const myStack = (sim: Partial<StackSeed>) =>
  prStack(
    7,
    [
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
        reviewDecision: "CHANGES_REQUESTED",
        assignees: ["mira-k"],
        reviews: [{ author: "mira-k", state: "CHANGES_REQUESTED" }],
        unresolvedThreads: 2,
      }),
      inboxPr({ number: 418, title: "maki over ACP", isDraft: true, headRef: "feat/maki-acp", checks: "FAILURE" }),
    ],
    {
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
      ...sim,
    },
  );

const theirStack = () =>
  prStack(
    8,
    [
      inboxPr({
        number: 401,
        title: "Focus rings on every control",
        author: "jo-r",
        headRef: "jo-r/focus-rings",
        headRepo: "jo-r/codex-custom",
        maintainerCanModify: true,
        reviewDecision: "REVIEW_REQUIRED",
        reviewRequests: ["ciaran"],
        reviews: [{ author: "ciaran", state: "APPROVED", commit: sha(400) }],
      }),
      inboxPr({
        number: 403,
        title: "Trap focus in dialogs",
        author: "jo-r",
        headRef: "jo-r/focus-trap",
        headRepo: "jo-r/codex-custom",
        reviewDecision: "REVIEW_REQUIRED",
        assignees: ["ciaran"],
        reviewRequests: ["ciaran"],
        checks: "PENDING",
      }),
    ],
    { behind: 1, commits: { 401: 3, 403: 7 } },
  );

const singles = () => [
  inboxPr({
    number: 128,
    title: "Add pull-request review view",
    headRef: "feature/pr-review",
    reviewDecision: "CHANGES_REQUESTED",
    reviews: [{ author: "octo-helper", state: "CHANGES_REQUESTED" }],
    unresolvedThreads: 2,
  }),
  inboxPr({
    number: 126,
    title: "Native git worktree service",
    author: "octo-helper",
    isDraft: true,
    headRef: "feature/worktrees",
    reviewDecision: "REVIEW_REQUIRED",
    reviewRequests: ["ciaran"],
    checks: "PENDING",
  }),
  inboxPr({
    number: 409,
    title: "Retry the app-server handshake",
    author: "mira-k",
    headRef: "mira-k/handshake-retry",
    reviewDecision: "CHANGES_REQUESTED",
    reviewRequests: ["ciaran"],
    reviews: [{ author: "tomasz", state: "CHANGES_REQUESTED" }],
    unresolvedThreads: 2,
  }),
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

/** Seed the desk with the scenario repository, signed in as `ciaran`
 *  (also `ciaran-work`); `myStack` changes how stack 7 answers. */
export function seedInboxScenario(myStackSim: Partial<StackSeed> = {}): void {
  const prs = singles();
  const [threaded, theirs] = [prs[2], theirStack().prs];
  seedPreviewDesk({
    repositories: [
      deskRepository(SLUG, {
        repository: { path: SCENARIO_DIR, key: SCENARIO_DIR, name: "codex-custom" },
        prs,
        stacks: [myStack(myStackSim), theirStack()],
        people: PEOPLE,
        events: [{ number: 126, kind: "reviewRequested", actor: null, state: null }],
        feed: [
          { number: 415, kind: "reviewed", actor: "mira-k", state: "CHANGES_REQUESTED", at: minutesAgo(12) },
          { number: 418, kind: "checksFailed", actor: null, state: null, at: minutesAgo(60) },
          { number: 401, kind: "pushed", actor: "jo-r", state: null, at: minutesAgo(120) },
          { number: 401, kind: "reviewRequested", actor: null, state: null, at: minutesAgo(120) },
          { number: 412, kind: "reviewed", actor: "mira-k", state: "APPROVED", at: minutesAgo(180) },
        ],
        commitsSinceMyReview: { 401: 2 },
        behind: { 128: 2 },
      }),
    ],
    agentReviews: [
      agentReview(SCENARIO_DIR, SLUG, threaded, { id: "review-409", status: "done", findings: 3, startedAt: 1 }),
      agentReview(SCENARIO_DIR, SLUG, theirs[0], { id: "review-401", status: "running", startedAt: 2 }),
      agentReview(SCENARIO_DIR, SLUG, theirs[1], { id: "review-403" }),
    ],
    github: { account: { login: "ciaran", tokenSource: "keyring", logins: ["ciaran", "ciaran-work"] } },
  });
}
