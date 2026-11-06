import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import BranchList from "$lib/components/BranchList.svelte";
import type { GitBranch } from "$lib/types";

const branch = (name: string, extra: Partial<GitBranch> = {}): GitBranch => ({
  name,
  isRemote: false,
  isCurrent: false,
  ...extra,
});

describe("BranchList", () => {
  it("lists remote-only branches under their own heading and picks on click", async () => {
    const onPick = vi.fn();
    render(BranchList, {
      id: "branches",
      label: "Filter branches",
      branches: [branch("main", { isCurrent: true }), branch("feature")],
      remote: [branch("origin/remote-only", { isRemote: true })],
      onPick,
    });
    expect(screen.getByText("Remote")).toBeTruthy();
    expect(screen.getByRole("option", { name: /main/ })).toBeDisabled();
    await userEvent.click(screen.getByRole("option", { name: /origin\/remote-only/ }));
    expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ name: "origin/remote-only" }));
  });

  it("drives the list from the keyboard as a combobox", async () => {
    const user = userEvent.setup();
    const onPick = vi.fn();
    render(BranchList, {
      id: "branches",
      label: "Existing branch",
      combobox: true,
      branches: [branch("main"), branch("feature/a"), branch("feature/b")],
      onPick,
    });
    const input = screen.getByRole("combobox", { name: "Existing branch" });
    await user.type(input, "feature");
    expect(screen.queryByRole("option", { name: /main/ })).toBeNull();
    await user.keyboard("{ArrowDown}");
    expect(input).toHaveAttribute("aria-activedescendant", "branches-option-1");
    await user.keyboard("{Enter}");
    expect(onPick).toHaveBeenCalledWith(expect.objectContaining({ name: "feature/b" }));
  });

  it("says when nothing matches the filter", async () => {
    const user = userEvent.setup();
    render(BranchList, { id: "branches", label: "Filter branches", branches: [branch("main")], onPick: vi.fn() });
    await user.type(screen.getByRole("searchbox", { name: "Filter branches" }), "zzz");
    expect(screen.getByText("No branch matches “zzz”.")).toBeTruthy();
  });
});
