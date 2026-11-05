// Agent reviews for the browser preview and the desk UI tests: runs,
// Findings and repository review rules held in memory, with the backend's
// queue limit (`review/desk/agent_reviews.rs`) and a small reading of the
// findings contract. `resetPreviewAgentReviews` puts the seeded runs back.

import type { AgentReview, AgentReviewRequest, Finding, ReviewRule } from "$lib/types";
import type { PreviewCommands } from "./commands";
import { deskInbox, deskPrDetail, deskRepo, previewAgentReviews, THREADED_PR } from "./desk";

/** How many run at once, as `agent_review::MAX_RUNNING`. */
const MAX_RUNNING = 2;
const MAIN = "/Users/ciaran/Projects/codex-custom";

const state = {
  reviews: [] as AgentReview[],
  findings: [] as Finding[],
  rules: new Map<string, ReviewRule[]>(),
  next: 0,
};

const slugOf = (repoDir: string) => deskRepo(repoDir).slug ?? repoDir;

/** Findings of the finished review on the threaded PR: one on a changed
 *  line, one merged into tomasz's comment, one outside the diff. */
function seededFindings(reviewId: string, headSha: string): Finding[] {
  const base = {
    agentReviewId: reviewId,
    prNumber: THREADED_PR,
    headSha,
    side: "RIGHT",
    endLine: null,
    evidence: null,
    sameAsCommentId: null,
    state: "draft" as const,
    dismissReason: null,
    dismissNote: null,
  };
  return [
    {
      ...base,
      id: `${reviewId}-1`,
      path: "src/lib/loader.ts",
      line: 7,
      severity: "medium",
      title: "Read errors lose the path",
      body: "readFileSync throws ENOENT without saying which file it was reading, so a missing config reads as a crash.",
      evidence: "loader.ts:7 calls readFileSync(path) with no try/catch.",
    },
    {
      ...base,
      id: `${reviewId}-2`,
      path: "src/lib/loader.ts",
      line: 6,
      severity: "low",
      title: "The original error is swallowed",
      body: "The thrown error replaces whatever went wrong first.",
      sameAsCommentId: 20,
      state: "merged",
    },
    {
      ...base,
      id: `${reviewId}-3`,
      path: "src/lib/loader.ts",
      line: 1,
      severity: "low",
      title: "Synchronous read on startup",
      body: "readFileSync blocks the event loop while the app starts.",
    },
  ];
}

/** Back to the seeded runs: a finished review with Findings on the threaded
 *  PR, plus the running and queued ones the inbox panel lists. */
export function resetPreviewAgentReviews(): void {
  const inbox = deskInbox(MAIN);
  const pr = (number: number) => inbox.prs.find((candidate) => candidate.number === number);
  state.reviews = previewAgentReviews.map((seed) => {
    const found = pr(seed.number);
    return review({
      id: seed.id,
      repoDir: MAIN,
      repo: seed.repo,
      number: seed.number,
      status: seed.status,
      startedAt: seed.startedAt,
      threadId: seed.threadId ?? null,
      findings: seed.findings ?? 0,
      finishedAt: seed.status === "done" ? seed.startedAt : null,
      request: {
        targets: [
          {
            number: seed.number,
            headRef: found?.headRef ?? "",
            headSha: found?.headSha ?? "",
            mine: false,
            stack: null,
          },
        ],
        harness: seed.harness,
        step: seed.step,
        focus: ["correctness", "tests"],
        keepDrafts: true,
        rerunOnNewCommits: false,
      },
      headSha: found?.headSha ?? "",
    });
  });
  const done = state.reviews.find((entry) => entry.number === THREADED_PR && entry.status === "done");
  state.findings = done ? seededFindings(done.id, done.headSha) : [];
  state.rules.clear();
  state.next = 0;
}

function review(fields: Partial<AgentReview> & Pick<AgentReview, "id" | "repoDir" | "number" | "request">) {
  return {
    repo: slugOf(fields.repoDir),
    status: "queued",
    threadId: null,
    headSha: fields.request.targets.at(-1)?.headSha ?? "",
    sinceSha: null,
    findings: 0,
    parseError: null,
    error: null,
    createdAt: Date.now(),
    startedAt: null,
    finishedAt: null,
    ...fields,
  } as AgentReview;
}

function find(id: string): AgentReview {
  const found = state.reviews.find((entry) => entry.id === id);
  if (!found) throw new Error("That agent review is gone");
  return found;
}

function enqueue(repoDir: string, request: AgentReviewRequest, sinceSha: string | null = null): AgentReview {
  if (request.targets.length === 0) throw new Error("Pick a pull request to review");
  if (request.focus.includes("runTests") && request.step === "noCheckout") {
    throw new Error("Running tests needs a review checkout");
  }
  const top = request.targets.at(-1);
  const created = review({
    id: `ar-preview-${++state.next}`,
    repoDir,
    number: top?.number ?? 0,
    request: structuredClone(request),
    sinceSha,
  });
  state.reviews.unshift(created);
  return structuredClone(created);
}

/** The last fenced `json` block of `message`, as the contract reads it. */
function parseBlock(
  message: string,
): (Omit<Finding, "id" | "agentReviewId" | "headSha" | "prNumber"> & { pr: number | null })[] | string {
  const blocks = [...message.matchAll(/```json\s*\n([\s\S]*?)\n```/g)];
  const last = blocks.at(-1);
  if (!last) return "The final message has no findings block";
  try {
    const listed = JSON.parse(last[1]) as Record<string, unknown>[];
    if (!Array.isArray(listed)) return "The findings block is malformed";
    return listed.map((entry) => ({
      pr: typeof entry.pr === "number" ? entry.pr : null,
      path: String(entry.path),
      line: Number(entry.line),
      endLine: (entry.endLine as number | undefined) ?? null,
      side: entry.side === "LEFT" ? "LEFT" : "RIGHT",
      severity: (entry.severity as Finding["severity"]) ?? "low",
      title: String(entry.title ?? ""),
      body: String(entry.body ?? ""),
      evidence: (entry.evidence as string | undefined) ?? null,
      sameAsCommentId: (entry.sameAsCommentId as number | undefined) ?? null,
      state: entry.sameAsCommentId ? "merged" : "draft",
      dismissReason: null,
      dismissNote: null,
    }));
  } catch {
    return "The findings block is malformed";
  }
}

/** No runs, Findings or rules at all, for tests of the queue. */
export function clearPreviewAgentReviews(): void {
  state.reviews = [];
  state.findings = [];
  state.rules.clear();
}

/** Seed a run, for tests. */
export function setPreviewAgentReview(entry: AgentReview, findings: Finding[] = []): void {
  state.reviews = [entry, ...state.reviews.filter((other) => other.id !== entry.id)];
  state.findings = [...state.findings.filter((other) => other.agentReviewId !== entry.id), ...findings];
}

export const previewAgentReviewCommands: PreviewCommands = {
  reviewEnqueueAgentReview: (repoDir, request) => enqueue(repoDir, request),
  reviewAgentReviews: (repoDir) =>
    structuredClone(state.reviews.filter((entry) => entry.repo === slugOf(repoDir)).slice(0, 50)),
  reviewActiveAgentReviews: () =>
    structuredClone(state.reviews.filter((entry) => entry.status === "queued" || entry.status === "running").reverse()),
  reviewClaimAgentReviews() {
    const oldestFirst = [...state.reviews].reverse();
    const running = oldestFirst.filter((entry) => entry.status === "running").length;
    const claimed = oldestFirst
      .filter((entry) => entry.status === "queued")
      .slice(0, Math.max(0, MAX_RUNNING - running));
    for (const entry of claimed) {
      entry.status = "running";
      entry.startedAt = Date.now();
    }
    return structuredClone(claimed);
  },
  reviewAgentReviewStarted(_repoDir, id, threadId) {
    find(id).threadId = threadId;
  },
  reviewAgentReviewPrompt(repoDir, id) {
    const entry = find(id);
    const detail = deskPrDetail(repoDir, entry.number);
    return `Review pull request #${entry.number} "${detail.summary.title}". End with a fenced JSON block of findings.`;
  },
  reviewRecordAgentReviewResult(_repoDir, id, message, error) {
    const entry = find(id);
    entry.finishedAt = Date.now();
    if (message == null) {
      entry.status = "failed";
      entry.error = error ?? "The agent review stopped";
      return structuredClone(entry);
    }
    entry.status = "done";
    const parsed = parseBlock(message);
    if (typeof parsed === "string") {
      entry.parseError = parsed;
      entry.findings = 0;
    } else {
      // As the desk files them: under the PR a Finding names, at its head.
      const findings = parsed.map(({ pr, ...finding }, index) => {
        const covered = entry.request.targets.find((target) => target.number === pr);
        return {
          ...finding,
          id: `${id}-${index + 1}`,
          agentReviewId: id,
          headSha: covered?.headSha ?? entry.headSha,
          prNumber: covered?.number ?? entry.number,
        };
      });
      state.findings = [...state.findings.filter((other) => other.agentReviewId !== id), ...findings];
      entry.findings = findings.length;
    }
    return structuredClone(entry);
  },
  reviewRerunAgentReview(repoDir, id, targets) {
    const previous = find(id);
    const request = {
      ...structuredClone(previous.request),
      targets: targets.length ? targets : previous.request.targets,
    };
    const head = request.targets.at(-1)?.headSha ?? "";
    return enqueue(repoDir, request, head !== previous.headSha ? previous.headSha : null);
  },
  reviewRerunOnNewCommits: () => [],
  reviewRemoveAgentReview(_repoDir, id) {
    state.reviews = state.reviews.filter((entry) => entry.id !== id);
    state.findings = state.findings.filter((finding) => finding.agentReviewId !== id);
  },
  reviewFindings: (repoDir, number) => {
    const slug = slugOf(repoDir);
    const runs = new Set(state.reviews.filter((entry) => entry.repo === slug).map((entry) => entry.id));
    return structuredClone(
      state.findings.filter((finding) => runs.has(finding.agentReviewId) && finding.prNumber === number),
    );
  },
  reviewSetFindingState(_repoDir, id, findingState, reason, note) {
    const finding = state.findings.find((entry) => entry.id === id);
    if (!finding) throw new Error("That finding is gone");
    finding.state = findingState;
    finding.dismissReason = findingState === "dismissed" ? (reason ?? "notAProblem") : null;
    finding.dismissNote = findingState === "dismissed" && note?.trim() ? note : null;
    return structuredClone(finding);
  },
  reviewRules: (repoDir) => structuredClone(state.rules.get(slugOf(repoDir)) ?? []),
  reviewAddRule(repoDir, rule) {
    const text = rule.trim();
    if (!text) throw new Error("Write the rule first");
    const created: ReviewRule = { id: `rule-${++state.next}`, rule: text, createdAt: Date.now() };
    state.rules.set(slugOf(repoDir), [...(state.rules.get(slugOf(repoDir)) ?? []), created]);
    return { ...created };
  },
  reviewUpdateRule(repoDir, id, rule) {
    const rules = state.rules.get(slugOf(repoDir)) ?? [];
    state.rules.set(
      slugOf(repoDir),
      rules.map((entry) => (entry.id === id ? { ...entry, rule: rule.trim() } : entry)),
    );
  },
  reviewRemoveRule(repoDir, id) {
    state.rules.set(
      slugOf(repoDir),
      (state.rules.get(slugOf(repoDir)) ?? []).filter((entry) => entry.id !== id),
    );
  },
};

resetPreviewAgentReviews();
