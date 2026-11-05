import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { BootstrapData, HomeSnapshot, ProfileHome } from "$lib/bindings";
import ProfileHomes from "$lib/layout/ProfileHomes.svelte";
import { acceptProfile } from "$lib/services/homeRouting";

vi.mock("$lib/services/tauri", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/services/tauri")>()),
  isTauri: () => true,
}));

const readHomeStatus = vi.fn();
const updateProfileHome = vi.fn(
  async (_id: string, _dir: string, _binary: string, _label: string) => ({}) as ProfileHome,
);
const removeProfileHome = vi.fn(async (_id: string) => null);
const bootstrapProfile = vi.fn(async () => ({ profileKey: "p", homes: snapshots() }));

vi.mock("$lib/bindings", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/bindings")>();
  return {
    ...actual,
    commands: {
      ...actual.commands,
      readHomeStatus: (id: string) => readHomeStatus(id),
      updateProfileHome: (id: string, configDir: string, binary: string, label: string) =>
        updateProfileHome(id, configDir, binary, label),
      removeProfileHome: (id: string) => removeProfileHome(id),
      bootstrapProfile: () => bootstrapProfile(),
    },
  };
});

const empty: BootstrapData = {
  codexHome: "/config",
  codexBinary: "codex",
  account: null,
  sideQuestions: [],
  threadBranches: [],
  subagents: [],
  unfiledThreads: [],
  sections: [],
  sectionsSupported: false,
  sidebarLayout: { folders: [], placements: [] },
  projects: [],
};

function snapshot(id: string, configDir: string, isDefault: boolean): HomeSnapshot {
  return {
    homeKey: id,
    home: {
      id,
      host: { kind: "native" },
      harness: "claude",
      configDir,
      binary: "claude",
      label: "Claude Code",
      isDefault,
      sharesProfileDb: false,
    },
    data: empty,
    error: null,
  };
}

function snapshots(): HomeSnapshot[] {
  return [snapshot("old", "/home/u/.claude", true), snapshot("personal", "/home/u/.claude-personal", false)];
}

function status(loggedIn: boolean | null) {
  return {
    available: true,
    path: "/usr/local/bin/claude",
    version: "2.1.251",
    configDir: "/home/u/.claude",
    loggedIn,
    protocolFloor: "1.0.59",
    message: null,
  };
}

function rowFor(configDir: string): HTMLElement {
  const rows = screen.getAllByTestId("home-row");
  const row = rows.find((candidate) => within(candidate).queryByTitle(configDir));
  if (!row) throw new Error(`No Home row for ${configDir}`);
  return row;
}

beforeEach(() => {
  acceptProfile({ profileKey: "p", homes: snapshots() });
  readHomeStatus.mockImplementation(async (id: string) => status(id === "personal"));
});

afterEach(() => {
  vi.clearAllMocks();
});

describe("ProfileHomes", () => {
  it("shows each Claude Home's login once its probe answers", async () => {
    render(ProfileHomes);
    // The list paints before the probes return.
    for (const badge of screen.getAllByTestId("home-login")) expect(badge).toHaveTextContent("Checking…");
    await waitFor(() => {
      expect(within(rowFor("/home/u/.claude")).getByTestId("home-login")).toHaveTextContent("Not logged in");
    });
    expect(within(rowFor("/home/u/.claude-personal")).getByTestId("home-login")).toHaveTextContent("Logged in");
  });

  it("says so when the login cannot be checked", async () => {
    readHomeStatus.mockImplementation(async () => status(null));
    render(ProfileHomes);
    await waitFor(() => {
      expect(within(rowFor("/home/u/.claude")).getByTestId("home-login")).toHaveTextContent("Login unknown");
    });
  });

  it("repoints a Home through the inline editor", async () => {
    const user = userEvent.setup();
    render(ProfileHomes);
    const row = rowFor("/home/u/.claude");
    // The fields only appear once the row is opened for editing.
    expect(within(row).queryByLabelText("Configuration directory")).not.toBeInTheDocument();
    await user.click(within(row).getByTestId("home-edit"));

    const directory = within(row).getByLabelText("Configuration directory");
    expect(directory).toHaveValue("/home/u/.claude");
    await user.clear(directory);
    await user.type(directory, "/home/u/.claude-work");
    await user.click(within(row).getByRole("button", { name: "Save Home" }));

    expect(updateProfileHome).toHaveBeenCalledWith("old", "/home/u/.claude-work", "claude", "Claude Code");
  });

  it("needs a second click before removing a Home", async () => {
    const user = userEvent.setup();
    render(ProfileHomes);
    const remove = within(rowFor("/home/u/.claude")).getByTestId("home-remove");
    await user.click(remove);
    expect(removeProfileHome).not.toHaveBeenCalled();
    expect(remove).toHaveTextContent("Confirm remove");
    expect(screen.getByText(/Nothing is deleted from disk/)).toBeInTheDocument();

    await user.click(remove);
    expect(removeProfileHome).toHaveBeenCalledWith("old");
  });

  it("reports an edit the backend refuses next to its row", async () => {
    const user = userEvent.setup();
    updateProfileHome.mockRejectedValue(new Error("Another Home already uses /home/u/.claude-personal"));
    render(ProfileHomes);
    const row = rowFor("/home/u/.claude");
    await user.click(within(row).getByTestId("home-edit"));
    await user.click(within(row).getByRole("button", { name: "Save Home" }));
    expect(await within(row).findByRole("alert")).toHaveTextContent("Another Home already uses");
  });
});
