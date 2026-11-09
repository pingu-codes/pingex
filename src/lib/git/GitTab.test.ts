import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { closeAllDialogs } from "$lib/app/dialogs.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import { commands } from "$lib/services/homeRouting";
import type { GitContext } from "$lib/types";
import GitTab from "./GitTab.svelte";

vi.mock("$lib/services/homeRouting", () => ({
  commands: {
    gitStatus: vi.fn(async () => ({
      branch: "pr/12",
      detached: false,
      upstream: null,
      ahead: 0,
      behind: 0,
      counts: { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 },
      files: [],
      truncated: false,
      refreshedAt: 0,
    })),
    gitContext: vi.fn(),
    gitPush: vi.fn(async () => ({ summary: "Pushed" })),
    reviewPushToPr: vi.fn(async () => undefined),
    reviewManagedWorktreeAt: vi.fn(async () => null),
  },
}));
vi.mock("$lib/services/codexEvents.svelte", () => ({ setThreadHandler: () => () => {} }));

const dir = "/home/.codex/pr-worktrees/repo/pr-12";
const context = { branch: "pr/12" } as GitContext;

function open() {
  render(DialogHost);
  render(GitTab, { props: { dir, context } });
}

describe("publishing from the Git tab", () => {
  beforeEach(() => {
    closeAllDialogs();
    vi.mocked(commands.gitPush).mockClear();
    vi.mocked(commands.reviewPushToPr).mockClear();
    vi.mocked(commands.reviewManagedWorktreeAt).mockResolvedValue(null);
  });

  it("publishes an ordinary branch to origin", async () => {
    open();
    await userEvent.click(await screen.findByRole("button", { name: "Publish branch" }));

    await waitFor(() => expect(commands.gitPush).toHaveBeenCalledWith(dir, true));
    expect(commands.reviewPushToPr).not.toHaveBeenCalled();
  });

  it("sends a managed PR worktree's commits to its PR after asking, never to origin", async () => {
    vi.mocked(commands.reviewManagedWorktreeAt).mockResolvedValue({ repoDir: "/repo", number: 12 });
    open();
    await userEvent.click(await screen.findByRole("button", { name: "Push to PR" }));
    expect(commands.reviewPushToPr).not.toHaveBeenCalled();

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("Push to #12?");
    await userEvent.click(screen.getAllByRole("button", { name: "Push to PR" }).at(-1) as HTMLElement);

    await waitFor(() => expect(commands.reviewPushToPr).toHaveBeenCalledWith("/repo", 12));
    expect(commands.gitPush).not.toHaveBeenCalled();
  });

  it("pushes nothing when that is cancelled", async () => {
    vi.mocked(commands.reviewManagedWorktreeAt).mockResolvedValue({ repoDir: "/repo", number: 12 });
    open();
    await userEvent.click(await screen.findByRole("button", { name: "Push to PR" }));
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

    expect(commands.reviewPushToPr).not.toHaveBeenCalled();
    expect(commands.gitPush).not.toHaveBeenCalled();
  });
});
