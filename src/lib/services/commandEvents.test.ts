import { afterEach, describe, expect, it, vi } from "vitest";
import { argNamed, onCommandSettled, type SettledCommand } from "$lib/services/commandEvents";
import { commands } from "$lib/services/homeRouting";

// Outside Tauri the proxy answers from the browser preview's fixtures, the
// same branch Playwright drives.
describe("the commands proxy announces settled git and review commands", () => {
  let stop: (() => void) | null = null;
  afterEach(() => stop?.());

  it("with the arguments and their names, after the command settles", async () => {
    const seen: SettledCommand[] = [];
    stop = onCommandSettled((command) => seen.push(command));
    const pending = commands.gitStatus("/repo").catch(() => null);
    expect(seen).toEqual([]);
    await pending;
    expect(seen).toHaveLength(1);
    expect(seen[0].name).toBe("gitStatus");
    expect(argNamed(seen[0], "dir")).toBe("/repo");
    expect(argNamed(seen[0], "missing")).toBeUndefined();
  });

  it("also when the command fails, and not for other commands", async () => {
    const listener = vi.fn();
    stop = onCommandSettled(listener);
    await commands.reviewPrDetail("/no-such-repo", 999_999).catch(() => null);
    await commands.listModels().catch(() => null);
    expect(listener.mock.calls.map(([command]) => command.name)).toEqual(["reviewPrDetail"]);
  });

  it("a listener that throws does not fail the command", async () => {
    stop = onCommandSettled(() => {
      throw new Error("broken cache");
    });
    const outcome = await commands.gitStatus("/repo").then(
      () => "settled",
      () => "settled",
    );
    expect(outcome).toBe("settled");
  });
});
