// @vitest-environment node
import { describe, expect, it } from "vitest";
import { can } from "$lib/utils/capabilities";

describe("can", () => {
  it("reads the drivers' declared capabilities", () => {
    expect(can("codex", "goals")).toBe(true);
    expect(can("claude", "goals")).toBe(false);
    expect(can("claude", "permissionModes")).toBe(true);
    expect(can("codex", "permissionModes")).toBe(false);
  });

  it("treats a missing harness as Codex and an unknown one as capable of nothing", () => {
    expect(can(null, "revert")).toBe(true);
    expect(can("some-acp-agent", "revert")).toBe(false);
  });
});
