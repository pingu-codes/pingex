import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { FileCounts } from "$lib/review/reviewPage";
import type { PrFile } from "$lib/types";
import FileRail from "./FileRail.svelte";

const file = (path: string, status = "modified"): PrFile => ({
  path,
  oldPath: null,
  status,
  additions: 1,
  deletions: 0,
  patch: "",
  hunks: [],
  patchTruncated: false,
  blobSha: null,
});

const files = [file("src/lib/a.ts"), file("src/lib/deep/b.ts"), file("README.md", "added")];

function setup(props: { currentPath?: string | null; collapsedDirs?: string[] } = {}) {
  const onJump = vi.fn();
  const view = render(FileRail, {
    files,
    currentPath: props.currentPath ?? null,
    countsByPath: new Map<string, FileCounts>([["src/lib/a.ts", { comments: 2, unresolved: 1, findings: 0 }]]),
    viewed: new Set(["README.md"]),
    onJump,
    collapsedDirs: props.collapsedDirs ?? [],
  });
  return { ...view, onJump };
}

const nextFrame = () => new Promise((resolve) => requestAnimationFrame(resolve));

/** A tree row by its visible name. */
const row = (name: string) => screen.getByText(name).closest('[role="treeitem"]') as HTMLElement;

describe("FileRail", () => {
  beforeEach(() => localStorage.clear());

  it("shows the files as a tree of folders, folding single-folder runs", () => {
    setup();
    expect(screen.getByRole("navigation", { name: "Changed files" })).toBeInTheDocument();
    expect(screen.getByText("src/lib")).toBeVisible();
    expect(screen.getByText("deep")).toBeVisible();
    expect(screen.getByText("a.ts")).toBeVisible();
    expect(screen.getByText("b.ts")).toBeVisible();
    expect(screen.getByLabelText("Viewed")).toBeInTheDocument();
    expect(screen.getByTitle("Comments")).toHaveTextContent("2");
  });

  it("folds a folder when its row is clicked, and opens it again", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByText("deep"));
    expect(screen.queryByText("b.ts")).not.toBeVisible();
    expect(screen.getByText("a.ts")).toBeVisible();
    await user.click(screen.getByText("deep"));
    expect(screen.getByText("b.ts")).toBeVisible();
  });

  it("jumps to a file when its row is clicked, even the current one", async () => {
    const user = userEvent.setup();
    const { onJump } = setup({ currentPath: "src/lib/a.ts" });
    await user.click(screen.getByText("b.ts"));
    expect(onJump).toHaveBeenLastCalledWith("src/lib/deep/b.ts");
    await user.click(screen.getByText("a.ts"));
    expect(onJump).toHaveBeenLastCalledWith("src/lib/a.ts");
  });

  it("marks the current file", () => {
    setup({ currentPath: "README.md" });
    expect(row("README.md")).toHaveAttribute("aria-current", "true");
    expect(row("a.ts")).not.toHaveAttribute("aria-current");
  });

  it("opens the folders above the current file", () => {
    setup({ currentPath: "src/lib/deep/b.ts", collapsedDirs: ["src/lib", "src/lib/deep"] });
    expect(screen.getByText("b.ts")).toBeVisible();
  });

  it("leaves the rail's scroll alone when a folder folds", async () => {
    const user = userEvent.setup();
    const { container } = setup({ currentPath: "README.md" });
    const rail = container.querySelector("[data-rail-scroll]") as HTMLElement;
    // The current row sits below the rail's view, as when the reader has
    // scrolled the rail up to fold a folder far from the file they're on.
    const rect = (top: number, bottom: number) => ({ top, bottom }) as DOMRect;
    vi.spyOn(rail, "getBoundingClientRect").mockReturnValue(rect(0, 100));
    vi.spyOn(row("README.md"), "getBoundingClientRect").mockReturnValue(rect(400, 420));
    await nextFrame();
    rail.scrollTop = 50;
    await user.click(screen.getByText("deep"));
    await nextFrame();
    expect(rail.scrollTop).toBe(50);
  });

  it("collapses and expands every folder at once", async () => {
    const user = userEvent.setup();
    setup();
    await user.click(screen.getByRole("button", { name: "Collapse all folders" }));
    expect(screen.queryByText("a.ts")).not.toBeVisible();
    expect(screen.getByText("README.md")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Expand all folders" }));
    expect(screen.getByText("b.ts")).toBeVisible();
  });

  it("folds to a strip and remembers it", async () => {
    const user = userEvent.setup();
    const { unmount } = setup();
    await user.click(screen.getByRole("button", { name: "Collapse files" }));
    expect(screen.queryByRole("navigation", { name: "Changed files" })).not.toBeInTheDocument();
    unmount();
    setup();
    expect(screen.queryByRole("navigation", { name: "Changed files" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand files" }));
    expect(screen.getByRole("navigation", { name: "Changed files" })).toBeInTheDocument();
  });

  it("starts at the width last dragged to", () => {
    localStorage.setItem("layout.reviewRailWidth", "333");
    setup();
    const nav = screen.getByRole("navigation", { name: "Changed files" });
    expect(nav.parentElement).toHaveStyle({ width: "333px" });
    expect(screen.getByRole("separator", { name: "Resize files" })).toBeInTheDocument();
  });
});
