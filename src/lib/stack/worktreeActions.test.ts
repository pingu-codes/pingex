import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import DialogHost from "$lib/components/DialogHost.svelte";
import { SCENARIO_DIR, seedInboxScenario } from "$lib/inbox/inboxScenario";
import { inboxes, syncInbox } from "$lib/inbox/inboxStore.svelte";
import { commands } from "$lib/services/homeRouting";
import { worktreeEntry } from "$lib/services/preview/factories";
import { worktreeAction } from "$lib/stack/worktreeActions";
import { toaster } from "$lib/toaster";
import type { Project } from "$lib/types";
import { worktreeMap } from "$lib/worktrees/worktreeMap.svelte";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      reviewSwitchStackBranch: vi.fn(actual.commands.reviewSwitchStackBranch),
      threadsWithActiveTurns: vi.fn(async () => []),
      copyText: vi.fn(async () => null),
    },
  };
});

const MAIN = SCENARIO_DIR;
const PATH = "/Users/ciaran/worktrees/codex-custom/harness-stack";
const project = { path: PATH, name: "harness-stack", kind: "worktree", threads: [] } as unknown as Project;

beforeEach(async () => {
  vi.clearAllMocks();
  seedInboxScenario();
  for (const key of Object.keys(inboxes)) delete inboxes[key];
  await syncInbox(MAIN);
  worktreeMap.byRepo = { [MAIN]: [worktreeEntry({ path: PATH, branch: "feat/claude-driver" })] };
});

describe("the sidebar worktree menu", () => {
  it("switches a stack's worktree to another of its PRs", async () => {
    render(DialogHost);
    const user = userEvent.setup();

    const done = worktreeAction("switchToPr", project);
    const dialog = await screen.findByRole("dialog");
    const choices = within(dialog).getByRole("radiogroup", { name: "Switch to PR" });
    expect(within(choices).getAllByRole("radio")).toHaveLength(3);
    expect(within(choices).getByRole("radio", { name: /#415 feat\/claude-driver/ })).toBeChecked();
    await user.click(within(choices).getByRole("radio", { name: /#418 feat\/maki-acp/ }));
    await user.click(within(dialog).getByRole("button", { name: "Switch" }));
    await done;

    // Through the desk, which refuses a dirty worktree, never a raw git switch.
    await waitFor(() => expect(commands.reviewSwitchStackBranch).toHaveBeenCalledWith(MAIN, 418));
    expect((await commands.reviewPrCheckout(MAIN, 418))?.onPrBranch).toBe(true);
  });

  describe("waiting for running threads", () => {
    async function chooseSwitch() {
      render(DialogHost);
      const user = userEvent.setup();
      const done = worktreeAction("switchToPr", project);
      const dialog = await screen.findByRole("dialog");
      await user.click(within(dialog).getByRole("radio", { name: /#418 feat\/maki-acp/ }));
      await user.click(within(dialog).getByRole("button", { name: "Switch" }));
      return { done };
    }

    it("doesn't check out when it can't tell what's running, and stops when asked", async () => {
      vi.mocked(commands.threadsWithActiveTurns).mockResolvedValueOnce([]).mockRejectedValue("no answer");
      vi.mocked(commands.reviewSwitchStackBranch).mockClear();
      const create = vi.spyOn(toaster, "create");
      const { done } = await chooseSwitch();

      const stop = await waitFor(
        () => {
          const action = create.mock.calls.map(([toast]) => toast.action).find(Boolean);
          if (!action) throw new Error("no waiting toast yet");
          return action;
        },
        { timeout: 5000 },
      );
      expect(commands.reviewSwitchStackBranch).not.toHaveBeenCalled();

      vi.mocked(commands.threadsWithActiveTurns).mockReset().mockResolvedValue([]);
      stop.onClick?.();
      await done;
      expect(commands.reviewSwitchStackBranch).not.toHaveBeenCalled();
      create.mockRestore();
    }, 10_000);
  });

  it("refuses to switch a worktree with uncommitted changes, and says why", async () => {
    render(DialogHost);
    const user = userEvent.setup();
    const failure = vi.spyOn(toaster, "error");
    vi.mocked(commands.reviewSwitchStackBranch).mockRejectedValueOnce(
      new Error(`${PATH} has uncommitted changes. Commit or stash them, then switch`),
    );

    const done = worktreeAction("switchToPr", project);
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("radio", { name: /#418 feat\/maki-acp/ }));
    await user.click(within(dialog).getByRole("button", { name: "Switch" }));
    await done;

    expect(failure).toHaveBeenCalledWith(
      expect.objectContaining({ description: expect.stringMatching(/uncommitted/) }),
    );
    expect((await commands.reviewPrCheckout(MAIN, 418))?.onPrBranch).toBe(false);
    failure.mockRestore();
  });

  it("copies the worktree's path", async () => {
    await worktreeAction("copyPath", project);
    expect(commands.copyText).toHaveBeenCalledWith(PATH);
  });
});
