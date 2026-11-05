// @vitest-environment node
import { describe, expect, it } from "vitest";
import { DESK_ACTION_STEPS } from "$lib/bindings";
import { ACTION_LABELS, stepFor, stepHint, stepUp } from "$lib/review/actions";

describe("the action requirement table", () => {
  it("labels exactly the actions the backend's table lists", () => {
    expect(Object.keys(ACTION_LABELS).sort()).toEqual(Object.keys(DESK_ACTION_STEPS).sort());
  });

  // The Model board's "What each action needs".
  it("needs the step the Model board gives each action", () => {
    expect(stepFor("comment")).toBe("noCheckout");
    expect(stepFor("merge")).toBe("noCheckout");
    expect(stepFor("diffReview")).toBe("noCheckout");
    expect(stepFor("agentReviewWithTests")).toBe("reviewCheckout");
    expect(stepFor("openInEditor")).toBe("reviewCheckout");
    expect(stepFor("fixCi")).toBe("worktree");
    expect(stepFor("push")).toBe("worktree");
  });

  it("steps a PR up only as far as the action needs", () => {
    expect(stepUp("noCheckout", "runLocally")).toBe("reviewCheckout");
    expect(stepUp("reviewCheckout", "restack")).toBe("worktree");
    expect(stepUp("worktree", "runLocally")).toBeNull();
    expect(stepUp("noCheckout", "approve" as never)).toBeNull();
  });

  it("names the step a button would create, and nothing when the PR has enough", () => {
    expect(stepHint("noCheckout", "runLocally")).toBe("Creates a review checkout");
    expect(stepHint("reviewCheckout", "fixInThread")).toBe("Creates a worktree");
    expect(stepHint("noCheckout", "submitReview")).toBeNull();
  });
});
