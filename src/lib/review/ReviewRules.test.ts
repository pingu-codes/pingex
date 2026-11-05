import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import ReviewRules from "$lib/review/ReviewRules.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewAgentReviews } from "$lib/services/preview/agentReviews";
import { DESK_REPOS } from "$lib/services/preview/desk";

const repoDir = DESK_REPOS[0].path as string;
const rules = async () => (await commands.reviewRules(repoDir)).map((rule) => rule.rule);

beforeEach(() => resetPreviewAgentReviews());

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
