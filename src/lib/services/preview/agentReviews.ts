// Agent reviews for the browser preview and the desk UI tests: runs,
// Findings and repository review rules on the preview desk, with the
// backend's queue limit (`review/desk/agent_reviews.rs`) and a small reading
// of the findings contract.

import type { AgentReview, AgentReviewRequest, Finding, ReviewRule } from "$lib/types";
import type { DeskCommands } from "./commands";
import { desk, deskPrDetail, deskRepo } from "./desk";

/** How many run at once, as `agent_review::MAX_RUNNING`. */
const MAX_RUNNING = 2;

const slugOf = (repoDir: string) => deskRepo(repoDir)?.slug || repoDir;

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
  const found = desk().agentReviews.find((entry) => entry.id === id);
  if (!found) throw new Error("That agent review is gone");
  return found;
}

function enqueue(repoDir: string, request: AgentReviewRequest, sinceSha: string | null = null): AgentReview {
  if (request.targets.length === 0) throw new Error("Pick a pull request to review");
  const top = request.targets.at(-1);
  // The agent works in the PR's worktree, as `agent_reviews::enqueue` insists.
  if (top && !desk().checkouts.has(`${repoDir}#${top.number}`)) {
    throw new Error(`Check out #${top.number} before an agent review`);
  }
  const created = review({
    id: `ar-preview-${++desk().next}`,
    repoDir,
    number: top?.number ?? 0,
    request: structuredClone(request),
    sinceSha,
  });
  desk().agentReviews.unshift(created);
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

export const agentReviewCommands = {
  reviewEnqueueAgentReview: (repoDir, request) => enqueue(repoDir, request),
  reviewAgentReviews: (repoDir) =>
    structuredClone(
      desk()
        .agentReviews.filter((entry) => entry.repo === slugOf(repoDir))
        .slice(0, 50),
    ),
  reviewActiveAgentReviews: () =>
    structuredClone(
      desk()
        .agentReviews.filter((entry) => entry.status === "queued" || entry.status === "running")
        .reverse(),
    ),
  reviewClaimAgentReviews() {
    const oldestFirst = [...desk().agentReviews].reverse();
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
    return null;
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
      desk().findings = [...desk().findings.filter((other) => other.agentReviewId !== id), ...findings];
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
    desk().agentReviews = desk().agentReviews.filter((entry) => entry.id !== id);
    desk().findings = desk().findings.filter((finding) => finding.agentReviewId !== id);
    return null;
  },
  reviewFindings: (repoDir, number) => {
    const slug = slugOf(repoDir);
    const runs = new Set(
      desk()
        .agentReviews.filter((entry) => entry.repo === slug)
        .map((entry) => entry.id),
    );
    return structuredClone(
      desk().findings.filter((finding) => runs.has(finding.agentReviewId) && finding.prNumber === number),
    );
  },
  reviewSetFindingState(_repoDir, id, findingState, reason, note) {
    const finding = desk().findings.find((entry) => entry.id === id);
    if (!finding) throw new Error("That finding is gone");
    finding.state = findingState;
    finding.dismissReason = findingState === "dismissed" ? (reason ?? "notAProblem") : null;
    finding.dismissNote = findingState === "dismissed" && note?.trim() ? note : null;
    return structuredClone(finding);
  },
  reviewRules: (repoDir) => structuredClone(desk().rules.get(slugOf(repoDir)) ?? []),
  reviewAddRule(repoDir, rule) {
    const text = rule.trim();
    if (!text) throw new Error("Write the rule first");
    const created: ReviewRule = { id: `rule-${++desk().next}`, rule: text, createdAt: Date.now() };
    desk().rules.set(slugOf(repoDir), [...(desk().rules.get(slugOf(repoDir)) ?? []), created]);
    return { ...created };
  },
  reviewUpdateRule(repoDir, id, rule) {
    const rules = desk().rules.get(slugOf(repoDir)) ?? [];
    desk().rules.set(
      slugOf(repoDir),
      rules.map((entry) => (entry.id === id ? { ...entry, rule: rule.trim() } : entry)),
    );
    return null;
  },
  reviewRemoveRule(repoDir, id) {
    desk().rules.set(
      slugOf(repoDir),
      (desk().rules.get(slugOf(repoDir)) ?? []).filter((entry) => entry.id !== id),
    );
    return null;
  },
} satisfies Partial<DeskCommands>;
