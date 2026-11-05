import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import Host from "./ActionMenu.testHost.svelte";
import type { MenuEntry } from "./actionMenu";

// The Pull request row menu from the canvas's Context menus board.
const ENTRIES: MenuEntry[] = [
  { id: "open", label: "Open", shortcut: "Enter" },
  {
    kind: "submenu",
    id: "openIn",
    label: "Open in worktree",
    entries: [
      { id: "worktree:harness-stack", label: "harness-stack", checked: true },
      { id: "worktree:main", label: "Main checkout", hint: "on main" },
      { kind: "separator" },
      { id: "worktree:new", label: "New worktree for #415" },
    ],
  },
  { id: "newThread", label: "New thread on this PR", shortcut: "T" },
  { id: "agentReview", label: "Start agent review", shortcut: "R", disabled: true },
  { kind: "separator" },
  { id: "copyBranch", label: "Copy branch name", shortcut: "CmdOrCtrl+Shift+C" },
  { kind: "separator" },
  { id: "close", label: "Close pull request…", danger: true },
];

function setup() {
  const onSelect = vi.fn();
  render(Host, { entries: ENTRIES, onSelect });
  return { onSelect, user: userEvent.setup() };
}

describe("ActionMenu", () => {
  it("opens the same menu from the ⋯ button and from a right-click, with shortcut hints", async () => {
    const { user } = setup();

    await user.click(screen.getByRole("button", { name: "More actions" }));
    const fromButton = screen.getByRole("menu", { name: "Pull request 415" });
    expect(within(fromButton).getByRole("menuitem", { name: /Open ↵/ })).toBeVisible();
    expect(within(fromButton).getByRole("menuitem", { name: /Copy branch name ⌘⇧C/ })).toBeVisible();
    const labels = within(fromButton)
      .getAllByRole("menuitem")
      .map((item) => item.textContent?.trim());
    await user.keyboard("{Escape}");

    await user.pointer({ keys: "[MouseRight]", target: screen.getByText("Claude Code driver over stream-json") });
    const fromRightClick = screen.getByRole("menu", { name: "Pull request 415" });
    expect(
      within(fromRightClick)
        .getAllByRole("menuitem")
        .map((item) => item.textContent?.trim()),
    ).toEqual(labels);
  });

  it("reports the chosen item, in the menu and in a submenu", async () => {
    const { onSelect, user } = setup();

    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: /New thread on this PR/ }));
    expect(onSelect).toHaveBeenLastCalledWith("newThread");

    await user.click(screen.getByRole("button", { name: "More actions" }));
    await user.click(screen.getByRole("menuitem", { name: /Open in worktree/ }));
    const submenu = await screen.findByRole("menu", { name: "Open in worktree" });
    await user.click(within(submenu).getByRole("menuitem", { name: /Main checkout/ }));
    expect(onSelect).toHaveBeenLastCalledWith("worktree:main");
  });

  it("runs an item's shortcut on the focused row without opening the menu", async () => {
    const { onSelect, user } = setup();

    screen.getByTestId("row").focus();
    await user.keyboard("t");
    expect(onSelect).toHaveBeenLastCalledWith("newThread");
    await user.keyboard("{Meta>}{Shift>}c{/Shift}{/Meta}");
    expect(onSelect).toHaveBeenLastCalledWith("copyBranch");
    await user.keyboard("{Enter}");
    expect(onSelect).toHaveBeenLastCalledWith("open");
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("leaves keys alone when focus is inside the row or the item is disabled", async () => {
    const { onSelect, user } = setup();

    await user.click(screen.getByRole("textbox", { name: "Note" }));
    await user.keyboard("t");
    screen.getByTestId("row").focus();
    await user.keyboard("r");

    expect(onSelect).not.toHaveBeenCalled();
  });
});
