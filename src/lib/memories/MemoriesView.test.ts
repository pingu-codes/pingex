import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MemoryHome } from "$lib/types";

const mocks = vi.hoisted(() => ({
  listMemories: vi.fn(),
  readMemory: vi.fn(),
  writeMemory: vi.fn(),
  deleteMemory: vi.fn(),
  setMemoryToggle: vi.fn(),
  revealInFinder: vi.fn(),
}));

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return { ...actual, commands: { ...actual.commands, ...mocks } };
});

import MemoriesView from "$lib/memories/MemoriesView.svelte";

function homes(useMemories = true, withRoadmap = true): MemoryHome[] {
  return [
    {
      homeId: "claude",
      label: "Claude Code",
      configDir: "/cfg",
      error: null,
      toggles: [{ key: "autoMemoryEnabled", label: "Auto memory", description: "Remember things", value: useMemories }],
      stores: [
        {
          root: "/cfg/projects/-app/memory",
          title: "app",
          projectPath: "/app",
          entries: [
            {
              path: "/cfg/projects/-app/memory/MEMORY.md",
              relPath: "MEMORY.md",
              title: "MEMORY",
              description: "Memory index",
              kind: "index",
              modifiedAt: null,
              bytes: 40,
              unindexed: false,
            },
            ...(withRoadmap
              ? [
                  {
                    path: "/cfg/projects/-app/memory/roadmap.md",
                    relPath: "roadmap.md",
                    title: "roadmap",
                    description: "What is done",
                    kind: "project",
                    modifiedAt: null,
                    bytes: 20,
                    unindexed: false,
                  },
                ]
              : []),
          ],
        },
      ],
    },
  ];
}

describe("MemoriesView", () => {
  beforeEach(() => {
    for (const mock of Object.values(mocks)) mock.mockReset();
    mocks.listMemories.mockResolvedValue(homes());
    mocks.readMemory.mockImplementation(async (_home: string, path: string) =>
      path.endsWith("MEMORY.md") ? "# Index\n- [Roadmap](roadmap.md)" : "Roadmap body",
    );
  });

  it("opens the index first and renders its markdown", async () => {
    render(MemoriesView);
    expect(await screen.findByRole("heading", { name: "Index" })).toBeTruthy();
    expect(mocks.readMemory).toHaveBeenCalledWith("claude", "/cfg/projects/-app/memory/MEMORY.md");
  });

  it("edits and saves a memory", async () => {
    const user = userEvent.setup();
    mocks.writeMemory.mockResolvedValue(homes());
    render(MemoriesView);
    await user.click(await screen.findByRole("button", { name: /roadmap/ }));
    expect(await screen.findByText("Roadmap body")).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "Edit memory" }));
    const editor = screen.getByLabelText("Memory contents");
    await user.clear(editor);
    await user.type(editor, "New body");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() =>
      expect(mocks.writeMemory).toHaveBeenCalledWith(
        "claude",
        "/cfg/projects/-app/memory/roadmap.md",
        "New body",
        expect.any(Array),
      ),
    );
    expect(await screen.findByText("New body")).toBeTruthy();
  });

  it("deletes after confirming and falls back to the index", async () => {
    const user = userEvent.setup();
    mocks.deleteMemory.mockResolvedValue(homes(true, false));
    render(MemoriesView);
    await user.click(await screen.findByRole("button", { name: /roadmap/ }));
    await user.click(screen.getByRole("button", { name: "Delete memory" }));
    expect(screen.getByText(/and its index line/)).toBeTruthy();
    await user.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() =>
      expect(mocks.deleteMemory).toHaveBeenCalledWith(
        "claude",
        "/cfg/projects/-app/memory/roadmap.md",
        expect.any(Array),
      ),
    );
    expect(screen.queryByRole("button", { name: /roadmap/ })).toBeNull();
  });

  it("flips a switch", async () => {
    const user = userEvent.setup();
    mocks.setMemoryToggle.mockResolvedValue(homes(false));
    render(MemoriesView);
    const toggle = await screen.findByRole("switch", { name: "Claude Code: Auto memory" });
    expect(toggle).toBeChecked();
    await user.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(mocks.setMemoryToggle).toHaveBeenCalledWith("claude", "autoMemoryEnabled", false, expect.any(Array));
  });
});
