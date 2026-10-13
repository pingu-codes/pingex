import { describe, expect, it } from "vitest";
import { commands } from "$lib/services/homeRouting";

// Outside Tauri the routed `commands` answer from the preview fakes.
describe("browser preview commands", () => {
  it("answers from the fixtures and resolves undefined for commands it does not fake", async () => {
    expect(await commands.sideQuestionMode("1")).toBe("fork");
    expect(await commands.listWslDistros()).toEqual([]);
    expect(await commands.killAgentRun("run-1")).toBeUndefined();
  });

  it("moves an unarchived thread back into its project", async () => {
    const archived = async () =>
      (await commands.listThreadsPage(null, 50, true, null)).items.map((thread) => thread.id);
    expect(await archived()).toContain("archived-1");

    const data = await commands.unarchiveThread("archived-1");

    const project = data.projects.find((project) => project.path === "/Users/ciaran/Projects/codex-custom");
    expect(project?.threads.map((thread) => thread.id)).toContain("archived-1");
    expect(await archived()).not.toContain("archived-1");
  });
});
