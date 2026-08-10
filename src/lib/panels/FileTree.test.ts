import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  listProjectFiles: vi.fn(),
}));

vi.mock("$lib/services/api", () => ({
  listProjectFiles: mocks.listProjectFiles,
}));

import FileTree from "$lib/panels/FileTree.svelte";

function setup() {
  const onOpenFile = vi.fn();
  render(FileTree, { root: "/project", onOpenFile });
  return { onOpenFile };
}

describe("FileTree", () => {
  // Braces matter: returning the mock from the arrow would make Vitest call
  // it as an after-test cleanup function.
  beforeEach(() => {
    mocks.listProjectFiles.mockReset();
  });

  it("lists directories before files and hides children until expanded", async () => {
    const user = userEvent.setup();
    mocks.listProjectFiles.mockResolvedValue(["zebra.md", "src/main.ts", "src/lib/api.ts"]);
    setup();

    expect(await screen.findByText("src")).toBeInTheDocument();
    const rows = screen.getAllByRole("button").map((button) => button.textContent?.trim());
    expect(rows).toEqual(["src", "zebra.md"]);
    expect(screen.queryByText("main.ts")).not.toBeInTheDocument();

    await user.click(screen.getByText("src"));
    expect(screen.getByText("lib")).toBeInTheDocument();
    expect(screen.getByText("main.ts")).toBeInTheDocument();
    expect(screen.queryByText("api.ts")).not.toBeInTheDocument();

    await user.click(screen.getByText("lib"));
    expect(screen.getByText("api.ts")).toBeInTheDocument();
  });

  it("reports the relative path when a file is opened", async () => {
    const user = userEvent.setup();
    mocks.listProjectFiles.mockResolvedValue(["src/lib/api.ts"]);
    const { onOpenFile } = setup();

    await user.click(await screen.findByText("src"));
    await user.click(screen.getByText("lib"));
    await user.click(screen.getByText("api.ts"));
    expect(onOpenFile).toHaveBeenCalledWith("src/lib/api.ts");
  });

  it("shows an empty state for a project without files", async () => {
    mocks.listProjectFiles.mockResolvedValue([]);
    setup();

    expect(await screen.findByText("No files in this project.")).toBeInTheDocument();
  });

  it("surfaces listing errors", async () => {
    mocks.listProjectFiles.mockRejectedValue(new Error("walk failed"));
    setup();

    expect(await screen.findByText("walk failed")).toBeInTheDocument();
  });
});
