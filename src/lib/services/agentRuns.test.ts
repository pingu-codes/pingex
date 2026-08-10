import { beforeEach, describe, expect, it } from "vitest";
import {
  type AgentRunEvent,
  activityFor,
  activityLabel,
  agentRuns,
  applyAgentActivity,
  applyAgentRunEvent,
  elapsedLabel,
  isRunning,
  resetAgentRuns,
  runByCallId,
  runForToolCall,
  runningCount,
  runsFor,
  setAgentRuns,
} from "$lib/services/agentRuns.svelte";
import type { AgentRun } from "$lib/types";

beforeEach(() => resetAgentRuns());

const run = (patch: Partial<AgentRun> = {}): AgentRun => ({
  runId: "agt_1",
  parentThreadId: "thread-1",
  parentTurnId: "turn-1",
  callId: "call-1",
  childThreadId: "child-1",
  name: "probe",
  prompt: "go",
  cwd: "/repo",
  model: null,
  reasoningEffort: null,
  status: "running",
  result: null,
  error: null,
  createdAt: 1,
  finishedAt: null,
  ...patch,
});

const event = (patch: Partial<AgentRunEvent> = {}): AgentRunEvent => ({
  runId: "agt_1",
  parentThreadId: "thread-1",
  callId: "call-1",
  childThreadId: "child-1",
  name: "probe",
  status: "running",
  result: null,
  error: null,
  ...patch,
});

describe("agent run store", () => {
  it("keeps runs per thread", () => {
    setAgentRuns("thread-1", [run()]);
    setAgentRuns("thread-2", [run({ runId: "agt_2", parentThreadId: "thread-2" })]);

    expect(runsFor("thread-1")).toHaveLength(1);
    expect(runsFor("thread-2")[0].runId).toBe("agt_2");
    expect(runsFor("thread-missing")).toEqual([]);
    expect(runsFor(null)).toEqual([]);
  });

  it("counts only the agents still working", () => {
    setAgentRuns("thread-1", [
      run({ runId: "a", status: "running" }),
      run({ runId: "b", status: "done" }),
      run({ runId: "c", status: "failed" }),
      run({ runId: "d", status: "killed" }),
      run({ runId: "e", status: "orphaned" }),
    ]);
    expect(runningCount("thread-1")).toBe(1);
  });

  it("keeps what the events know when a stored row has not caught up", () => {
    // The row is written asynchronously, so a refresh on a thread switch can
    // read it back before it has learned the child thread id or the outcome.
    // Taking it verbatim drops the agent out of the side menu, which is the
    // one thing a refresh must never do.
    applyAgentRunEvent(event({ childThreadId: "child-1", status: "done", result: "found it" }));

    setAgentRuns("thread-1", [run({ childThreadId: null, status: "running", result: null })]);

    const [merged] = runsFor("thread-1");
    expect(merged.childThreadId).toBe("child-1");
    expect(merged.status).toBe("done");
    expect(merged.result).toBe("found it");
  });

  it("takes the stored row's own values once it has them", () => {
    applyAgentRunEvent(event({ status: "running" }));
    setAgentRuns("thread-1", [run({ status: "failed", error: "it broke", childThreadId: "child-9" })]);

    const [merged] = runsFor("thread-1");
    expect(merged.status).toBe("failed");
    expect(merged.error).toBe("it broke");
    expect(merged.childThreadId).toBe("child-9");
    // Fields only the read can supply come across too.
    expect(merged.prompt).toBe("go");
  });

  it("keeps a working run the read missed, and drops a finished one", () => {
    applyAgentRunEvent(event({ runId: "agt_live", callId: "call-live", status: "running" }));
    applyAgentRunEvent(event({ runId: "agt_gone", callId: "call-gone", status: "done" }));

    setAgentRuns("thread-1", [run({ runId: "agt_stored" })]);

    expect(runsFor("thread-1").map((entry) => entry.runId)).toEqual(["agt_stored", "agt_live"]);
  });

  it("recognises terminal statuses", () => {
    expect(isRunning(run({ status: "running" }))).toBe(true);
    for (const status of ["done", "failed", "killed", "orphaned"]) {
      expect(isRunning(run({ status }))).toBe(false);
    }
  });
});

describe("applyAgentRunEvent", () => {
  it("updates the run it names", () => {
    setAgentRuns("thread-1", [run()]);

    applyAgentRunEvent(event({ status: "done", result: "all good" }));

    const [updated] = runsFor("thread-1");
    expect(updated.status).toBe("done");
    expect(updated.result).toBe("all good");
    expect(updated.finishedAt).not.toBeNull();
  });

  it("creates a run the store has not seen yet", () => {
    // The event can beat the listAgentRuns that would have introduced it, and
    // a card appearing late reads as a bug.
    applyAgentRunEvent(event({ runId: "agt_new", name: "fresh" }));

    const runs = runsFor("thread-1");
    expect(runs).toHaveLength(1);
    expect(runs[0].runId).toBe("agt_new");
    expect(runs[0].name).toBe("fresh");
  });

  it("never erases a result a previous event delivered", () => {
    setAgentRuns("thread-1", [run({ status: "running", result: "partial" })]);

    applyAgentRunEvent(event({ status: "killed", result: null }));

    expect(runsFor("thread-1")[0].result).toBe("partial");
  });

  it("keeps the child thread id once it is known", () => {
    setAgentRuns("thread-1", [run({ childThreadId: "child-1" })]);
    applyAgentRunEvent(event({ childThreadId: null, status: "done" }));
    expect(runsFor("thread-1")[0].childThreadId).toBe("child-1");
  });

  it("keeps the first finish time across later events", () => {
    setAgentRuns("thread-1", [run({ status: "done", finishedAt: 42 })]);
    applyAgentRunEvent(event({ status: "done" }));
    expect(runsFor("thread-1")[0].finishedAt).toBe(42);
  });

  it("ignores an event with no parent thread", () => {
    applyAgentRunEvent(event({ parentThreadId: "" }));
    expect(Object.keys(agentRuns.byThread)).toHaveLength(0);
  });
});

describe("activityLabel", () => {
  const started = (item: Record<string, unknown>) => ({
    method: "item/started",
    params: { threadId: "child-1", item },
  });

  it("names what the agent is doing", () => {
    expect(activityLabel(started({ type: "commandExecution", command: "rg -n foo" }))).toBe("$ rg -n foo");
    expect(activityLabel(started({ type: "webSearch", query: "turso busy" }))).toBe("searching “turso busy”");
    expect(activityLabel(started({ type: "reasoning" }))).toBe("thinking…");
    expect(activityLabel(started({ type: "agentMessage" }))).toBe("writing…");
    expect(activityLabel(started({ type: "dynamicToolCall", tool: "grep" }))).toBe("grep");
  });

  it("counts the other files in a multi-file edit", () => {
    const changes = [{ path: "a.rs" }, { path: "b.rs" }, { path: "c.rs" }];
    expect(activityLabel(started({ type: "fileChange", changes }))).toBe("editing a.rs +2 more");
    expect(activityLabel(started({ type: "fileChange", changes: [{ path: "a.rs" }] }))).toBe("editing a.rs");
  });

  it("reads the streaming events that arrive between items", () => {
    expect(activityLabel({ method: "turn/started", params: {} })).toBe("starting…");
    expect(activityLabel({ method: "item/agentMessage/delta", params: {} })).toBe("writing…");
    expect(activityLabel({ method: "item/reasoning/summaryTextDelta", params: {} })).toBe("thinking…");
  });

  it("leaves the last line alone for events that say nothing new", () => {
    // Command output arrives in a flood of deltas; replacing the line with
    // each one would say no more than the line already there.
    expect(activityLabel({ method: "item/commandExecution/outputDelta", params: {} })).toBeNull();
    expect(activityLabel(started({ type: "somethingNew" }))).toBeNull();
  });
});

describe("applyAgentActivity", () => {
  const working = (item: Record<string, unknown>, threadId = "child-1") => ({
    method: "item/started",
    params: { threadId, item },
  });
  const command = { type: "commandExecution", command: "rg -n foo" };

  it("shows what an agent is doing in the thread that spawned it", () => {
    setAgentRuns("thread-1", [run()]);
    applyAgentActivity(working(command));
    expect(activityFor(run())?.label).toBe("$ rg -n foo");
  });

  it("ignores threads that are not an agent's", () => {
    setAgentRuns("thread-1", [run()]);
    applyAgentActivity(working(command, "some-ordinary-thread"));
    expect(activityFor(run())).toBeNull();
  });

  it("holds activity that beats the event introducing the run", () => {
    // A spawn learns the child thread id and starts the turn in one breath, so
    // the child's first notifications can arrive before anything here knows
    // which run that thread belongs to.
    applyAgentActivity(working(command));
    expect(activityFor(run())).toBeNull();

    applyAgentRunEvent(event({ childThreadId: "child-1" }));

    expect(activityFor(run())?.label).toBe("$ rg -n foo");
  });

  it("stops when the turn ends, so a finished agent shows no stale line", () => {
    setAgentRuns("thread-1", [run()]);
    applyAgentActivity(working(command));
    applyAgentActivity({ method: "turn/completed", params: { threadId: "child-1" } });
    expect(activityFor(run())).toBeNull();
  });

  it("stops when the run reaches a terminal state", () => {
    setAgentRuns("thread-1", [run()]);
    applyAgentActivity(working(command));
    applyAgentRunEvent(event({ status: "done", result: "found it" }));
    expect(activityFor(run())).toBeNull();
  });

  it("restarts the clock on a follow-up turn but not on each new item", () => {
    setAgentRuns("thread-1", [run()]);
    applyAgentActivity({ method: "turn/started", params: { threadId: "child-1" } });
    const first = activityFor(run())?.since;

    applyAgentActivity(working(command));
    expect(activityFor(run())?.since).toBe(first);
    expect(activityFor(run())?.label).toBe("$ rg -n foo");

    applyAgentActivity({ method: "turn/completed", params: { threadId: "child-1" } });
    applyAgentActivity({ method: "turn/started", params: { threadId: "child-1" } });
    expect(activityFor(run())?.since).toBeGreaterThanOrEqual(first ?? 0);
    expect(activityFor(run())?.label).toBe("starting…");
  });
});

describe("elapsedLabel", () => {
  it("reads as seconds under a minute and minutes above", () => {
    expect(elapsedLabel(1_000, 13_000)).toBe("12s");
    expect(elapsedLabel(0, 64_000)).toBe("1m 04s");
    // A clock that has not ticked yet must not render a negative age.
    expect(elapsedLabel(9_000, 1_000)).toBe("0s");
  });
});

describe("a follow-up turn's result", () => {
  it("clears the answer it supersedes rather than showing it beside a working agent", () => {
    setAgentRuns("thread-1", [run({ status: "done", result: "the first answer" })]);

    applyAgentRunEvent(event({ status: "running", result: "" }));

    expect(runsFor("thread-1")[0].result).toBeNull();
  });
});

describe("runForToolCall", () => {
  it("matches on the call id when the transcript kept it", () => {
    setAgentRuns("thread-1", [run({ callId: "call-1", name: "probe" })]);
    expect(runForToolCall({ id: "call-1", arguments: { name: "probe" } })?.runId).toBe("agt_1");
  });

  it("falls back to the agent's name when the id was rewritten", () => {
    // `thread/read` renumbers items to `item-N` and drops dynamicToolCall
    // entirely, so a re-read can lose the id the join depends on. Without the
    // fallback the card renders inert: no status, no way into the thread.
    setAgentRuns("thread-1", [run({ callId: "call-1", name: "probe" })]);
    expect(runForToolCall({ id: "item-7", arguments: { name: "probe" } })?.runId).toBe("agt_1");
  });

  it("matches nothing when neither the id nor a name lines up", () => {
    setAgentRuns("thread-1", [run({ callId: "call-1", name: "probe" })]);
    expect(runForToolCall({ id: "item-7", arguments: { name: "other" } })).toBeNull();
    expect(runForToolCall({ id: "item-7" })).toBeNull();
    expect(runForToolCall({})).toBeNull();
  });
});

describe("runByCallId", () => {
  it("finds a run created by an event, not just a listed one", () => {
    // The event is what introduces a run spawned mid-turn. Without its callId
    // the transcript card can never match it, and renders unnamed and inert.
    applyAgentRunEvent(event({ runId: "agt_live", callId: "call-live", name: "sweep" }));

    expect(runByCallId("call-live")?.name).toBe("sweep");
  });

  it("finds a run across threads by the call that spawned it", () => {
    setAgentRuns("thread-1", [run({ callId: "call-1" })]);
    setAgentRuns("thread-2", [run({ runId: "agt_2", parentThreadId: "thread-2", callId: "call-2" })]);

    expect(runByCallId("call-2")?.runId).toBe("agt_2");
    expect(runByCallId("call-missing")).toBeNull();
    expect(runByCallId(null)).toBeNull();
    expect(runByCallId(undefined)).toBeNull();
  });
});
