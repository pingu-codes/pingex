import { describe, expect, it } from "vitest";
import { freshPlanPrompt } from "$lib/thread/planHandoff";

describe("freshPlanPrompt", () => {
  it("carries the whole plan into the message", () => {
    const plan = "## Ship it\n1. Write the code\n2. Test it";
    expect(freshPlanPrompt(plan)).toContain(plan);
  });

  it("tells the new thread it has no other context", () => {
    expect(freshPlanPrompt("do the thing")).toMatch(/only context/i);
  });

  it("trims surrounding whitespace from the plan", () => {
    expect(freshPlanPrompt("\n\n  step one\n\n")).toMatch(/step one$/);
  });
});
