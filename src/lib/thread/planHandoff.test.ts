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

/**
 * Golden copy of the fresh-thread prompt for the live e2e suite
 * (`src-tauri/tests/live_codex`), which replays it against a real codex with
 * `${PLAN}` substituted. Rerun with -u after changing the prompt.
 */
describe("plan handoff fixture", () => {
  it("matches the checked-in golden file (rerun with -u to regenerate)", async () => {
    // Spelled out so the linter does not read it as a template string.
    const generated = `${JSON.stringify({ freshPlanPrompt: freshPlanPrompt("${PLAN}") }, null, 2)}\n`;
    await expect(generated).toMatchFileSnapshot("../../../tests/fixtures/protocol/plan-handoff.json");
  });
});
