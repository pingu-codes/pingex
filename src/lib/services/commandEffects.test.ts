import { afterEach, describe, expect, it } from "vitest";
import { commandArguments } from "$lib/bindings";
import { EFFECTS, type Effect, isAnnounced, onEffect } from "$lib/services/commandEffects";
import { emitCommandSettled } from "$lib/services/commandEvents";
import { READS } from "$lib/services/queryCache.svelte";

describe("the command effects table", () => {
  // The `Record` type already fails `bun run check` on a missing command;
  // this holds if that guard is ever loosened.
  it("declares every git and review command, and nothing else", () => {
    const announced = Object.keys(commandArguments).filter((name) => /^(git|review)[A-Z]/.test(name));
    expect(Object.keys(EFFECTS).sort()).toEqual(announced.sort());
  });

  it("declares every cached read a read", () => {
    for (const name of Object.keys(READS)) expect(EFFECTS[name as keyof typeof READS].changes).toEqual([]);
  });

  it("announces only the commands it declares", () => {
    expect(isAnnounced("gitCommit")).toBe(true);
    expect(isAnnounced("listModels")).toBe(false);
  });
});

describe("onEffect", () => {
  let stop: (() => void) | null = null;
  afterEach(() => stop?.());

  const heard = () => {
    const effects: Effect[] = [];
    stop = onEffect((effect) => effects.push(effect));
    return effects;
  };

  it("names the repository and directories, and adds what a change implies", () => {
    const effects = heard();
    emitCommandSettled({
      name: "reviewReply",
      names: ["repoDir", "number", "commentId", "body", "window"],
      args: ["/repo", 1, 5, "ok", null],
    });
    expect(effects).toHaveLength(1);
    expect([...effects[0].changes].sort()).toEqual(["git", "github", "inbox"]);
    expect(effects[0].repo).toBe("/repo");
    expect(effects[0].paths).toEqual(["/repo"]);
  });

  it("collects every directory a command names", () => {
    const effects = heard();
    emitCommandSettled({
      name: "gitWorktreeHandoff",
      names: ["worktreePath", "targetDir", "commitUncommitted", "branchName", "window"],
      args: ["/wt", "/main", true, "b", null],
    });
    expect(effects[0].repo).toBeUndefined();
    expect(effects[0].paths).toEqual(["/wt", "/main"]);
  });

  it("stays quiet for reads and for commands it does not declare", () => {
    const effects = heard();
    emitCommandSettled({ name: "reviewWorktreeContext", names: ["dir", "window"], args: ["/repo", null] });
    emitCommandSettled({ name: "listModels", names: ["window"], args: [null] });
    expect(effects).toEqual([]);
  });
});
