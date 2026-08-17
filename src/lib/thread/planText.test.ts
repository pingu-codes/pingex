import { describe, expect, it } from "vitest";
import { planText } from "./planText";
import type { ThreadItem } from "$lib/types";

describe("planText", () => {
  it("reads a plan item", () => {
    expect(planText({ id: "1", type: "plan", text: "do it" } as ThreadItem)).toBe("do it");
  });

  it("reads a proposed_plan block out of a plain agent message", () => {
    const text = "Here you go\n<proposed_plan>\n# Title\n\nSteps\n</proposed_plan>\n";
    expect(planText({ id: "1", type: "agentMessage", text } as ThreadItem)).toBe("# Title\n\nSteps");
  });

  it("ignores messages without a plan and streaming ones", () => {
    expect(planText({ id: "1", type: "agentMessage", text: "hi" } as ThreadItem)).toBeNull();
    const text = "<proposed_plan>\nx\n</proposed_plan>";
    expect(planText({ id: "1", type: "agentMessage", text, streaming: true } as ThreadItem)).toBeNull();
  });
});
