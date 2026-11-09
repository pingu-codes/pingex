// @vitest-environment node
import { describe, expect, it } from "vitest";
import { DESK_ACTION_STEPS } from "$lib/bindings";
import { ACTION_LABELS, stepFor, stepHint, stepUp } from "$lib/review/actions";

describe("the action requirement table", () => {
  it("labels exactly the actions the backend's table lists", () => {
    expect(Object.keys(ACTION_LABELS).sort()).toEqual(Object.keys(DESK_ACTION_STEPS).sort());
  });

  // The Model board's "What each action needs".
  it("needs the step each action works at", () => {
    expect(stepFor("comment")).toBe("noCheckout");
    expect(stepFor("merge")).toBe("noCheckout");
    expect(stepFor("diffReview")).toBe("noCheckout");
    expect(stepFor("agentReviewWithTests")).toBe("worktree");
    expect(stepFor("openInEditor")).toBe("worktree");
    expect(stepFor("workOnPr")).toBe("worktree");
    expect(stepFor("fixCi")).toBe("worktree");
    expect(stepFor("push")).toBe("worktree");
  });

  it("steps a PR up only when the action needs it", () => {
    expect(stepUp("noCheckout", "runLocally")).toBe("worktree");
    expect(stepUp("noCheckout", "workOnPr")).toBe("worktree");
    expect(stepUp("worktree", "runLocally")).toBeNull();
    expect(stepUp("noCheckout", "approve" as never)).toBeNull();
  });

  it("names the step a button would create, and nothing when the PR has enough", () => {
    expect(stepHint("noCheckout", "runLocally")).toBe("Creates a worktree");
    expect(stepHint("worktree", "fixInThread")).toBeNull();
    expect(stepHint("noCheckout", "submitReview")).toBeNull();
  });
});
