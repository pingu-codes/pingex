import { beforeEach, describe, expect, it, vi } from "vitest";
import { scopedId } from "$lib/services/homeRouting";
import {
  applyProcessEvent as applyTyped,
  backgroundProcessCount,
  elapsedLabel,
  processByKey,
  processes,
  reconcileBackground,
  resetProcesses,
  runningProcessCount,
  stopAllBackground,
  terminateProcess,
} from "$lib/services/processes.svelte";

const api = vi.hoisted(() => ({
  listBackgroundTerminals: vi.fn(),
  terminateBackgroundTerminal: vi.fn(),
  cleanBackgroundTerminals: vi.fn(),
}));
vi.mock("$lib/services/api", () => api);

import { type FakeEvent, fakeEvent } from "$lib/testing/codexEvents";

const applyProcessEvent = (event: FakeEvent) => applyTyped(fakeEvent(event));

describe("elapsedLabel", () => {
  it("reads as seconds under a minute and minutes above", () => {
    expect(elapsedLabel(1_000, 13_000)).toBe("12s");
    expect(elapsedLabel(0, 64_000)).toBe("1m 04s");
    // A clock that has not ticked yet must not render a negative age.
    expect(elapsedLabel(9_000, 1_000)).toBe("0s");
  });
});

const started = (threadId: string, itemId: string, command = "sleep 60") => ({
  method: "item/started",
  params: { threadId, turnId: "turn-1", item: { type: "commandExecution", id: itemId, command, cwd: "/repo" } },
});

beforeEach(() => {
  resetProcesses();
  api.listBackgroundTerminals.mockReset().mockResolvedValue([]);
  api.terminateBackgroundTerminal.mockReset().mockResolvedValue(true);
  api.cleanBackgroundTerminals.mockReset().mockResolvedValue(undefined);
});

const startedWithPty = (threadId: string, itemId: string, processId: string, command = "bun run dev") => ({
  method: "item/started",
  params: {
    threadId,
    turnId: "turn-1",
    item: { type: "commandExecution", id: itemId, command, cwd: "/repo", processId },
  },
});

const terminal = (itemId: string, processId: string, command = "bun run dev") => ({
  itemId,
  processId,
  command,
  cwd: "/repo",
});

describe("background terminals", () => {
  it("keeps a command with a PTY session running past its turn while Codex lists it", async () => {
    api.listBackgroundTerminals.mockResolvedValue([terminal("c1", "1000")]);
    applyProcessEvent(startedWithPty("t", "c1", "1000"));
    applyProcessEvent(started("t", "c2"));
    applyProcessEvent({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });
    expect(processByKey("t:c1")).toMatchObject({ status: "running", background: true });
    expect(processByKey("t:c2")?.status).toBe("completed");
    await reconcileBackground("t");
    expect(api.listBackgroundTerminals).toHaveBeenCalledWith("t");
    expect(processByKey("t:c1")?.status).toBe("running");
    expect(backgroundProcessCount("t")).toBe(1);
  });

  it("finishes a background entry Codex no longer lists", async () => {
    applyProcessEvent(startedWithPty("t", "c1", "1000"));
    applyProcessEvent({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });
    await reconcileBackground("t");
    expect(processByKey("t:c1")?.status).toBe("completed");
  });

  it("finishes it normally when its exit arrives after the turn", async () => {
    api.listBackgroundTerminals.mockResolvedValue([terminal("c1", "1000")]);
    applyProcessEvent(startedWithPty("t", "c1", "1000"));
    applyProcessEvent({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });
    await reconcileBackground("t");
    applyProcessEvent({
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "commandExecution", id: "c1", exitCode: 0 } },
    });
    expect(processByKey("t:c1")).toMatchObject({ status: "completed", exitCode: 0 });
  });

  it("adds terminals this window never saw start", async () => {
    api.listBackgroundTerminals.mockResolvedValue([terminal("old", "7", "vite")]);
    await reconcileBackground("t");
    expect(processByKey("t:old")).toMatchObject({
      status: "running",
      background: true,
      processId: "7",
      command: "vite",
    });
  });

  it("leaves everything alone when the list cannot be read", async () => {
    api.listBackgroundTerminals.mockRejectedValue(new Error("Codex is not running"));
    applyProcessEvent(startedWithPty("t", "c1", "1000"));
    applyProcessEvent({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });
    await reconcileBackground("t");
    expect(processByKey("t:c1")?.status).toBe("running");
  });

  it("stops one terminal and keeps it reading as stopped when its exit lands", async () => {
    applyProcessEvent(startedWithPty("t", "c1", "1000"));
    const process = processByKey("t:c1")!;
    await expect(terminateProcess(process)).resolves.toBe(true);
    expect(api.terminateBackgroundTerminal).toHaveBeenCalledWith("t", "1000");
    expect(process.status).toBe("interrupted");
    applyProcessEvent({
      method: "item/completed",
      params: { threadId: "t", turnId: "turn-1", item: { type: "commandExecution", id: "c1", exitCode: 143 } },
    });
    expect(process.status).toBe("interrupted");
  });

  it("stops every background terminal of a thread", async () => {
    api.listBackgroundTerminals.mockResolvedValue([terminal("c1", "1"), terminal("c2", "2")]);
    applyProcessEvent(startedWithPty("t", "c1", "1"));
    applyProcessEvent(startedWithPty("t", "c2", "2"));
    applyProcessEvent({ method: "turn/completed", params: { threadId: "t", turn: { id: "turn-1" } } });
    await reconcileBackground("t");
    api.listBackgroundTerminals.mockResolvedValue([]);
    await stopAllBackground("t");
    expect(api.cleanBackgroundTerminals).toHaveBeenCalledWith("t");
    expect(backgroundProcessCount("t")).toBe(0);
  });
});

describe("applyProcessEvent", () => {
  it("disconnects only processes owned by the failed Home", () => {
    const windows = scopedId("windows", "same-thread");
    const linux = scopedId("linux", "same-thread");
    applyProcessEvent(started(windows, "same-item"));
    applyProcessEvent(started(linux, "same-item"));
    applyTyped({ method: "disconnected", params: null, homeKey: "linux" });
    expect(processByKey(`${windows}:same-item`)?.status).toBe("running");
    expect(processByKey(`${linux}:same-item`)?.status).toBe("interrupted");
  });
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
