// @vitest-environment node
import { describe, expect, it } from "vitest";
import { statusBucket, statusTone } from "$lib/thread/agentStatus";

describe("statusBucket", () => {
  it("buckets Codex agents, app runs and processes alike", () => {
    expect(["pendingInit", "running", "inProgress"].map(statusBucket)).toEqual(["Active", "Active", "Active"]);
    expect(["errored", "failed", "notFound"].map(statusBucket)).toEqual(["Failed", "Failed", "Failed"]);
    expect(["killed", "orphaned", "interrupted"].map(statusBucket)).toEqual(["Stopped", "Stopped", "Stopped"]);
    expect(["completed", "done", "shutdown"].map(statusBucket)).toEqual(["Finished", "Finished", "Finished"]);
  });

  it("reads an unknown status as finished rather than working", () => {
    expect(statusBucket("somethingNew")).toBe("Finished");
  });
});

describe("statusTone", () => {
  it("only colours working and failed states", () => {
    expect(statusTone("running")).toBe("text-success-500");
    expect(statusTone("errored")).toBe("text-error-500");
    expect(statusTone("killed")).toBe("text-surface-500");
    expect(statusTone("done")).toBe("text-surface-500");
  });
});
