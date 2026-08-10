import { beforeEach, describe, expect, it } from "vitest";
import {
  applyProcessEvent,
  processByKey,
  processes,
  resetProcesses,
  runningProcessCount,
} from "$lib/services/processes.svelte";

const started = (threadId: string, itemId: string, command = "sleep 60") => ({
  method: "item/started",
  params: { threadId, turnId: "turn-1", item: { type: "commandExecution", id: itemId, command, cwd: "/repo" } },
});

beforeEach(resetProcesses);

describe("applyProcessEvent", () => {
  it("registers a command on item/started and tracks it as running", () => {
    applyProcessEvent(started("t", "c1"));
    expect(processes.list).toHaveLength(1);
    expect(processes.list[0]).toMatchObject({
      key: "t:c1",
      threadId: "t",
      turnId: "turn-1",
      command: "sleep 60",
      cwd: "/repo",
      status: "running",
    });
    expect(runningProcessCount("t")).toBe(1);
  });

  it("appends output deltas and stdin to the mirrored output", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent({
      method: "item/commandExecution/outputDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "c1", delta: "hello " },
    });
    applyProcessEvent({
      method: "item/commandExecution/terminalInteraction",
      params: { threadId: "t", turnId: "turn-1", itemId: "c1", stdin: "y\n" },
    });
    expect(processByKey("t:c1")?.output).toBe("hello y\n");
  });

  it("registers a process when a delta beats item/started", () => {
    applyProcessEvent({
      method: "item/commandExecution/outputDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "c1", delta: "early" },
    });
    expect(processByKey("t:c1")?.output).toBe("early");
    applyProcessEvent(started("t", "c1"));
    expect(processes.list).toHaveLength(1);
    expect(processByKey("t:c1")?.command).toBe("sleep 60");
  });

  it("finalizes on item/completed with the exit code, keeping the longer output", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent({
      method: "item/commandExecution/outputDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "c1", delta: "streamed output" },
    });
    applyProcessEvent({
      method: "item/completed",
      params: {
        threadId: "t",
        turnId: "turn-1",
        item: { type: "commandExecution", id: "c1", command: "sleep 60", status: "completed", exitCode: 0 },
      },
    });
    const process = processByKey("t:c1");
    expect(process).toMatchObject({ status: "completed", exitCode: 0, output: "streamed output" });
    expect(process?.finishedAt).not.toBeNull();
  });

  it("marks a non-zero exit as failed", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent({
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "commandExecution", id: "c1", exitCode: 2 } },
    });
    expect(processByKey("t:c1")?.status).toBe("failed");
  });

  it("finalizes a thread's leftover processes when its turn completes", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent(started("other", "c2"));
    applyProcessEvent({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });
    expect(processByKey("t:c1")?.status).toBe("completed");
    expect(processByKey("other:c2")?.status).toBe("running");
  });

  it("fails a thread's processes on a non-retryable error but not a retryable one", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent({ method: "error", params: { threadId: "t", willRetry: true } });
    expect(processByKey("t:c1")?.status).toBe("running");
    applyProcessEvent({ method: "error", params: { threadId: "t", willRetry: false } });
    expect(processByKey("t:c1")?.status).toBe("failed");
  });

  it("finalizes on leaving review mode, which never sends turn/completed", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent({
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "exitedReviewMode", id: "r1" } },
    });
    expect(processByKey("t:c1")?.status).toBe("completed");
  });

  it("interrupts everything still running on disconnect", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent(started("other", "c2"));
    applyProcessEvent({ method: "disconnected", params: null });
    expect(processes.list.map((process) => process.status)).toEqual(["interrupted", "interrupted"]);
  });

  it("caps the mirrored output, keeping the tail", () => {
    applyProcessEvent(started("t", "c1"));
    applyProcessEvent({
      method: "item/commandExecution/outputDelta",
      params: { threadId: "t", turnId: "turn-1", itemId: "c1", delta: `${"x".repeat(250_000)}END` },
    });
    const output = processByKey("t:c1")!.output;
    expect(output.length).toBeLessThanOrEqual(200_002 + 3);
    expect(output.endsWith("END")).toBe(true);
    expect(output.startsWith("…")).toBe(true);
  });
});
