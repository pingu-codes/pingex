import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { StatusFile } from "$lib/types";
import StatusFileList from "./StatusFileList.svelte";

const files: StatusFile[] = [
  { path: "src/a.ts", state: "staged", code: "M." },
  { path: "src/b.ts", state: "unstaged", code: ".M" },
  { path: "notes.md", state: "untracked", code: "" },
];

describe("StatusFileList", () => {
  it("groups files and wires stage, unstage and discard per row", async () => {
    const onStage = vi.fn();
    const onUnstage = vi.fn();
    const onDiscard = vi.fn();
    render(StatusFileList, { files, onSelect: vi.fn(), onStage, onUnstage, onDiscard });

    expect(screen.getByRole("region", { name: "Staged" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Changes" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Untracked" })).toBeTruthy();

    await userEvent.click(screen.getByRole("button", { name: "Unstage src/a.ts" }));
    expect(onUnstage).toHaveBeenCalledWith(["src/a.ts"]);
    await userEvent.click(screen.getByRole("button", { name: "Stage src/b.ts" }));
    expect(onStage).toHaveBeenCalledWith(["src/b.ts"]);
    await userEvent.click(screen.getByRole("button", { name: "Discard notes.md" }));
    expect(onDiscard).toHaveBeenCalledWith([], ["notes.md"]);
    await userEvent.click(screen.getByRole("button", { name: "Discard src/b.ts" }));
    expect(onDiscard).toHaveBeenCalledWith(["src/b.ts"], []);
  });

  it("stages a whole section at once and reports the selected row", async () => {
    const onStage = vi.fn();
    const onSelect = vi.fn();
    render(StatusFileList, { files, onSelect, onStage, onUnstage: vi.fn(), onDiscard: vi.fn() });

    const changes = screen.getByRole("region", { name: "Changes" });
    await userEvent.click(changes.querySelector("button")!); // header "Stage all"
    expect(onStage).toHaveBeenCalledWith(["src/b.ts"]);

    await userEvent.click(screen.getByRole("button", { name: /^src\/a\.ts/ }));
    expect(onSelect).toHaveBeenCalledWith({ path: "src/a.ts", section: "staged" });
  });

  it("says the tree is clean when there is nothing to show", () => {
    render(StatusFileList, { files: [], onSelect: vi.fn(), onStage: vi.fn(), onUnstage: vi.fn(), onDiscard: vi.fn() });
    expect(screen.getByText("Working tree clean.")).toBeTruthy();
  });
});
