import { beforeEach, describe, expect, it, vi } from "vitest";
import { prLinks, resetPrLinks } from "$lib/review/prLinks.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewCheckouts, setPreviewCheckout } from "$lib/services/preview/checkouts";
import type { CheckoutTarget } from "$lib/types";
import { startPrThread } from "./prThreads";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      startThread: vi.fn(actual.commands.startThread),
      startTurn: vi.fn(actual.commands.startTurn),
    },
  };
});

const repoDir = "/Users/ciaran/Projects/codex-custom";
const target: CheckoutTarget = {
  number: 409,
  headRef: "jo-r/usage-meter",
  headSha: "4".repeat(40),
  mine: false,
  stack: null,
};
const input = [{ type: "text" as const, text: "Review this" }];

describe("startPrThread", () => {
  beforeEach(() => {
    resetPreviewCheckouts();
    resetPrLinks();
    vi.mocked(commands.startThread).mockClear();
    vi.mocked(commands.startTurn).mockClear();
  });

  it("starts a read-only thread in the clone at No checkout and records its PR link first", async () => {
    const { thread, link } = await startPrThread({
      repoDir,
      target,
      input,
      options: { sandboxMode: "workspace-write", approvalPolicy: "never" },
    });

    expect(commands.startThread).toHaveBeenCalledWith(repoDir, null, null);
    expect(vi.mocked(commands.startTurn).mock.calls[0][2]).toMatchObject({
      sandboxMode: "read-only",
      approvalPolicy: "on-request",
    });
    expect(link).toMatchObject({ threadId: thread.id, prNumber: 409, step: "noCheckout", startSha: target.headSha });
    expect(prLinks.byThread[thread.id]?.prNumber).toBe(409);
  });

  it("only reads in a stack's worktree that is on another PR's branch", async () => {
    setPreviewCheckout(repoDir, {
      number: 409,
      step: "worktree",
      path: "/wt/stack",
      headSha: target.headSha,
      gitBranch: "feat/top",
      setupPending: null,
      ahead: 0,
      behind: 0,
      dirty: false,
      managed: false,
      onPrBranch: false,
      mainCheckout: false,
    });

    await startPrThread({
      repoDir,
      target: { ...target, stack: "focus", stackBranches: ["jo-r/usage-meter", "feat/top"] },
      input,
      options: { sandboxMode: "danger-full-access" },
    });

    expect(commands.startThread).toHaveBeenCalledWith("/wt/stack", null, null);
    expect(vi.mocked(commands.startTurn).mock.calls[0][2]).toMatchObject({ sandboxMode: "read-only" });
  });

  it("runs in the worktree with the caller's permissions once the PR has one", async () => {
    setPreviewCheckout(repoDir, {
      number: 409,
      step: "worktree",
      path: "/wt/usage-meter",
      headSha: target.headSha,
      gitBranch: target.headRef,
      setupPending: null,
      ahead: 0,
      behind: 0,
      dirty: false,
      managed: false,
      onPrBranch: true,
      mainCheckout: false,
    });
    await startPrThread({ repoDir, target, input, options: { sandboxMode: "danger-full-access" } });

    expect(commands.startThread).toHaveBeenCalledWith("/wt/usage-meter", null, null);
    expect(vi.mocked(commands.startTurn).mock.calls[0][2]).toMatchObject({ sandboxMode: "danger-full-access" });
  });

  it("lets a run nobody chose permissions for write in the worktree, and nothing more", async () => {
    setPreviewCheckout(repoDir, {
      number: 409,
      step: "worktree",
      path: "/wt/pr-409",
      headSha: target.headSha,
      gitBranch: "pr/409",
      setupPending: null,
      ahead: 0,
      behind: 0,
      dirty: false,
      managed: true,
      onPrBranch: true,
      mainCheckout: false,
    });
    await startPrThread({ repoDir, target, input, harness: "maki" });

    expect(commands.startThread).toHaveBeenCalledWith("/wt/pr-409", null, "maki");
    expect(vi.mocked(commands.startTurn).mock.calls[0][2]).toMatchObject({
      approvalPolicy: "on-request",
      sandboxMode: "workspace-write",
    });
  });

  it("refuses a harness without a read-only sandbox at No checkout", async () => {
    await expect(startPrThread({ repoDir, target, input, harness: "maki" })).rejects.toThrow(/read-only sandbox/);
    expect(commands.startThread).not.toHaveBeenCalled();
  });
});
