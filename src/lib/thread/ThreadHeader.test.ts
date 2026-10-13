import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  handoffThreadLink: vi.fn(async () => "codex://thread/t-1"),
  copyText: vi.fn(async () => null),
}));

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return { ...actual, commands: { ...actual.commands, ...api } };
});
vi.mock("$lib/worktrees/gitStatus.svelte", () => ({
  ensureGitStatus: vi.fn(),
  refreshGitStatus: vi.fn(),
  gitStatusCache: { byPath: {}, loading: {}, fetchedAt: {} },
}));

import ThreadHeader from "$lib/thread/ThreadHeader.svelte";

function setup(threadId: string | null = "t-1") {
  render(ThreadHeader, {
    codexHome: "/home/.codex",
    repoName: "repo",
    repoDir: "/repo",
    cwd: "/repo",
    threadId,
    onMovedToWorktree: vi.fn(),
    onError: vi.fn(),
  });
}

function pressEscape(): KeyboardEvent {
  const event = new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true });
  document.activeElement?.dispatchEvent(event);
  return event;
}

describe("ThreadHeader handoff menu", () => {
  it("copies the thread link from the menu", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "Handoff" }));
    await user.click(await screen.findByRole("menuitem", { name: "Copy thread link" }));
    await waitFor(() => expect(api.copyText).toHaveBeenCalledWith("codex://thread/t-1"));
  });

  // The composer interrupts the running turn on an unclaimed Escape.
  it("claims Escape while open", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "Handoff" }));
    await screen.findByRole("menuitem", { name: "Move to worktree" });
    expect(pressEscape().defaultPrevented).toBe(true);
    await waitFor(() => expect(screen.queryByRole("menuitem", { name: "Move to worktree" })).not.toBeInTheDocument());
  });

  it("is disabled until the thread exists", () => {
    setup(null);
    expect(screen.getByRole("button", { name: "Handoff" })).toBeDisabled();
  });
});
