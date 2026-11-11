import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import ReviewRules from "$lib/review/ReviewRules.svelte";
import { commands } from "$lib/services/homeRouting";
import { seedPreviewDesk } from "$lib/services/preview/desk";
import { deskRepository } from "$lib/services/preview/factories";

const repoDir = "/repos/app";
const rules = async () => (await commands.reviewRules(repoDir)).map((rule) => rule.rule);

beforeEach(() => seedPreviewDesk({ repositories: [deskRepository("me/app", { repository: { path: repoDir } })] }));

describe("repository review rules", () => {
  it("adds, edits and removes the rules agent reviews skip", async () => {
    const user = userEvent.setup();
    await commands.reviewAddRule(repoDir, "Import order nits");
    render(ReviewRules, { repoDir });

    const rule = await screen.findByRole("textbox", { name: "Rule" });
    expect(rule).toHaveValue("Import order nits");
    await fireEvent.change(rule, { target: { value: "Import order" } });
    await waitFor(async () => expect(await rules()).toEqual(["Import order"]));

    await user.type(screen.getByRole("textbox", { name: "New rule" }), "Missing doc comments");
    await user.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(async () => expect(await rules()).toEqual(["Import order", "Missing doc comments"]));

    await user.click(screen.getByRole("button", { name: "Remove rule Import order" }));
    await waitFor(async () => expect(await rules()).toEqual(["Missing doc comments"]));
  });
});
