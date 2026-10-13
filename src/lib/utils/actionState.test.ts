import { describe, expect, it } from "vitest";
import { submitState } from "$lib/utils/actionState.svelte";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

describe("submitState", () => {
  it("reports failure inline instead of throwing", async () => {
    const action = submitState();

    expect(await action.run(async () => {})).toBe(true);
    expect(action.error).toBeNull();

    expect(
      await action.run(async () => {
        throw new Error("worktree already exists");
      }),
    ).toBe(false);
    expect(action.error).toBe("worktree already exists");
    expect(action.busy).toBe(false);
  });

  it("clears the previous error when a new run starts", async () => {
    const action = submitState();
    await action.run(async () => {
      throw new Error("first");
    });
    const gate = deferred();
    const running = action.run(() => gate.promise);
    expect(action.error).toBeNull();
    expect(action.busy).toBe(true);
    gate.resolve();
    expect(await running).toBe(true);
    expect(action.busy).toBe(false);
  });

  it("refuses a second run while one is in flight", async () => {
    const action = submitState();
    const gate = deferred();
    let calls = 0;
    const first = action.run(async () => {
      calls += 1;
      await gate.promise;
    });
    expect(
      await action.run(async () => {
        calls += 1;
      }),
    ).toBe(false);
    gate.resolve();
    expect(await first).toBe(true);
    expect(calls).toBe(1);
  });

  it("lets runs overlap when not exclusive, busy until the last settles", async () => {
    const action = submitState({ exclusive: false });
    const a = deferred();
    const b = deferred();
    const first = action.run(() => a.promise);
    const second = action.run(() => b.promise);
    a.resolve();
    expect(await first).toBe(true);
    expect(action.busy).toBe(true);
    b.resolve();
    expect(await second).toBe(true);
    expect(action.busy).toBe(false);
  });

  it("formats the error with `describe`", async () => {
    const action = submitState({ describe: (cause) => `Could not reply: ${String(cause)}` });
    await action.run(async () => {
      throw "timeout";
    });
    expect(action.error).toBe("Could not reply: timeout");
  });
});
