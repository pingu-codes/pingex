import { beforeEach, describe, expect, it, vi } from "vitest";
import { agentReviews, enqueueAgentReview, startStackReview } from "$lib/review/agentReviews.svelte";
import { previewEmit } from "$lib/services/codexEvents.svelte";
import { commands } from "$lib/services/homeRouting";
import { seedPreviewDesk } from "$lib/services/preview/desk";
import { deskRepository, inboxPr } from "$lib/services/preview/factories";
import type { AgentReviewRequest, CheckoutTarget, InboxPr, ThreadDetail } from "$lib/types";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      startThread: vi.fn(actual.commands.startThread),
      // The agent's turn ends when the test says, not when a fake stream does.
      startTurn: vi.fn(async () => ({ id: "turn-1", items: [], status: "inProgress" })),
      readThread: vi.fn(),
    },
  };
});

const repoDir = "/Users/ciaran/Projects/codex-custom";

function target(number: number): CheckoutTarget {
  return {
    number,
    headRef: `feature-${number}`,
    headSha: String(number).repeat(40).slice(0, 40),
    mine: false,
    stack: null,
  };
}

function request(number: number): AgentReviewRequest {
  return {
    targets: [target(number)],
    harness: "claude",
    step: "noCheckout",
    focus: ["correctness"],
    keepDrafts: true,
    rerunOnNewCommits: false,
  };
}

function finalMessage(threadId: string, text: string): ThreadDetail {
  return {
    id: threadId,
    preview: "",
    cwd: repoDir,
    turns: [{ id: "turn-1", status: "completed", items: [{ type: "agentMessage", id: "m1", text }] }],
  } as unknown as ThreadDetail;
}

const statuses = () => (agentReviews.byRepo[repoDir] ?? []).map((review) => `${review.number}:${review.status}`).sort();

describe("the agent review queue", () => {
  beforeEach(() => {
    // No agent reviews yet, and the PRs they review.
    const prs = [412, 415, 418].map((number) =>
      inboxPr({ number, title: `PR ${number}`, headRef: `feature-${number}` }),
    );
    seedPreviewDesk({ repositories: [deskRepository("ciaran/codex-custom", { repository: { path: repoDir }, prs })] });
    vi.mocked(commands.startThread).mockClear();
    vi.mocked(commands.startTurn).mockClear();
  });

  it("runs two reviews at a time and starts the next when one finishes with its findings", async () => {
    await enqueueAgentReview(repoDir, request(412));
    await enqueueAgentReview(repoDir, request(415));
    await enqueueAgentReview(repoDir, request(418));

    await vi.waitFor(() => expect(commands.startTurn).toHaveBeenCalledTimes(2));
    await vi.waitFor(() => expect(statuses()).toEqual(["412:running", "415:running", "418:queued"]));
    const prompt = vi.mocked(commands.startTurn).mock.calls[0][1] as { text: string }[];
    expect(prompt[0].text).toMatch(/#412/);

    const threadId = (await vi.mocked(commands.startThread).mock.results[0].value).id as string;
    vi.mocked(commands.readThread).mockResolvedValue(
      finalMessage(
        threadId,
        'One issue.\n\n```json\n[{"path": "src/a.ts", "line": 3, "severity": "high", "title": "Off by one", "body": "x"}]\n```',
      ) as never,
    );
    previewEmit({ method: "turn/completed", params: { threadId, turn: { id: "turn-1" } } } as never);

    await vi.waitFor(() => expect(statuses()).toEqual(["412:done", "415:running", "418:running"]));
    const done = agentReviews.byRepo[repoDir].find((review) => review.number === 412);
    expect(done?.findings).toBe(1);
    expect((await commands.reviewFindings(repoDir, 412)).map((finding) => finding.title)).toEqual(["Off by one"]);
  });

  it("puts findings straight into the pending review when they aren't kept as drafts", async () => {
    await enqueueAgentReview(repoDir, { ...request(412), keepDrafts: false });
    await vi.waitFor(() => expect(commands.startTurn).toHaveBeenCalledTimes(1));
    const threadId = (await vi.mocked(commands.startThread).mock.results[0].value).id as string;
    vi.mocked(commands.readThread).mockResolvedValue(
      finalMessage(
        threadId,
        '```json\n[{"path": "src/a.ts", "line": 3, "severity": "low", "title": "Typo", "body": "teh"}]\n```',
      ) as never,
    );
    previewEmit({ method: "turn/completed", params: { threadId, turn: { id: "turn-1" } } } as never);

    await vi.waitFor(async () => {
      const pending = await commands.reviewPendingReview(repoDir, 412);
      expect(pending?.entries).toEqual([expect.objectContaining({ kind: "acceptedFinding", body: "**Typo**\n\nteh" })]);
    });
    expect((await commands.reviewFindings(repoDir, 412))[0].state).toBe("added");
  });

  it.each(["failed", "interrupted"])("records a run whose turn ended %s as failed", async (status) => {
    await enqueueAgentReview(repoDir, request(412));
    await vi.waitFor(() => expect(commands.startTurn).toHaveBeenCalledTimes(1));
    const threadId = (await vi.mocked(commands.startThread).mock.results[0].value).id as string;
    // An earlier message held a findings block; the turn then failed.
    vi.mocked(commands.readThread).mockResolvedValue(
      finalMessage(
        threadId,
        '```json\n[{"path": "src/a.ts", "line": 3, "severity": "low", "title": "Typo", "body": "teh"}]\n```',
      ) as never,
    );
    previewEmit({ method: "turn/completed", params: { threadId, turn: { id: "turn-1", status } } } as never);

    await vi.waitFor(() => expect(statuses()).toEqual(["412:failed"]));
    expect(await commands.reviewFindings(repoDir, 412)).toEqual([]);
  });

  it("puts each stack Finding into its own PR's pending review", async () => {
    const targets = [412, 418].map(target);
    await enqueueAgentReview(repoDir, { ...request(418), targets, keepDrafts: false });
    await vi.waitFor(() => expect(commands.startTurn).toHaveBeenCalledTimes(1));
    const threadId = (await vi.mocked(commands.startThread).mock.results[0].value).id as string;
    vi.mocked(commands.readThread).mockResolvedValue(
      finalMessage(
        threadId,
        '```json\n[{"pr": 412, "path": "src/a.ts", "line": 3, "severity": "low", "title": "Low", "body": "a"},\n' +
          '{"pr": 418, "path": "src/b.ts", "line": 4, "severity": "low", "title": "High", "body": "b"}]\n```',
      ) as never,
    );
    previewEmit({ method: "turn/completed", params: { threadId, turn: { id: "turn-1" } } } as never);

    await vi.waitFor(async () => {
      const lower = await commands.reviewPendingReview(repoDir, 412);
      expect(lower?.entries).toEqual([expect.objectContaining({ kind: "acceptedFinding", body: "**Low**\n\na" })]);
      expect(lower?.headSha).toBe(target(412).headSha);
    });
    const upper = await commands.reviewPendingReview(repoDir, 418);
    expect(upper?.entries).toEqual([expect.objectContaining({ kind: "acceptedFinding", body: "**High**\n\nb" })]);
    expect((await commands.reviewFindings(repoDir, 412))[0].state).toBe("added");
  });

  it("reviews a whole stack as one unit filed under its top PR", async () => {
    const prs = [412, 415, 418].map(
      (number) =>
        ({ number, headRef: `feature-${number}`, headSha: target(number).headSha, author: "ciaran" }) as InboxPr,
    );
    const review = await startStackReview(repoDir, prs);
    expect(review.number).toBe(418);
    expect(review.request.targets.map((entry) => entry.number)).toEqual([412, 415, 418]);
    await vi.waitFor(() => expect(commands.startTurn).toHaveBeenCalledTimes(1));
  });
});
