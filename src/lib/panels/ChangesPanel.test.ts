import { fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";
import { beforeEach, expect, it, vi } from "vitest";
import type { ChangedFile, ChangesSummary, FileDiff } from "$lib/types";

const gitFileDiff = vi.fn();

vi.mock("$lib/services/api", () => ({
  gitFileDiff: (...args: unknown[]) => gitFileDiff(...args),
}));

import ChangesPanel from "$lib/panels/ChangesPanel.svelte";

function file(path: string): ChangedFile {
  return { path, oldPath: null, status: "modified", additions: 1, deletions: 0, binary: false } as ChangedFile;
}

const summary: ChangesSummary = {
  base: "HEAD",
  baseBranch: null,
  files: [file("a.ts"), file("b.ts")],
  truncated: false,
  totalFiles: 2,
  additions: 2,
  deletions: 0,
};

function patch(path: string, line: string): FileDiff {
  return { path, patch: `@@ -0,0 +1 @@\n+${line}\n`, truncated: false, binary: false, bytes: 20 };
}

/** Answers for `gitFileDiff`, released by the test in any order. */
function deferred() {
  let resolve!: (diff: FileDiff) => void;
  const promise = new Promise<FileDiff>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await tick();
}

beforeEach(() => {
  gitFileDiff.mockReset();
});

it("drops a slow diff for a file the user has moved past", async () => {
  const slowA = deferred();
  gitFileDiff.mockImplementation((_cwd: string, _base: string, path: string) =>
    path === "a.ts" ? slowA.promise : Promise.resolve(patch("b.ts", "bravo_line")),
  );
  const view = render(ChangesPanel, { cwd: "/repo", summary, focusPath: "a.ts", onRefresh: () => {} });
  await view.rerender({ focusPath: "b.ts" });
  expect(await screen.findByText(/bravo_line/)).toBeInTheDocument();

  slowA.resolve(patch("a.ts", "alpha_line"));
  await settle();
  expect(screen.queryByText(/alpha_line/)).not.toBeInTheDocument();
  expect(screen.getByText(/bravo_line/)).toBeInTheDocument();
});

it("keeps a cached diff on screen when an earlier request answers late", async () => {
  const slowA = deferred();
  gitFileDiff.mockImplementation((_cwd: string, _base: string, path: string) =>
    path === "a.ts" ? slowA.promise : Promise.resolve(patch("b.ts", "bravo_line")),
  );
  render(ChangesPanel, { cwd: "/repo", summary, onRefresh: () => {} });
  const back = () => fireEvent.click(screen.getByRole("button", { name: "Back to changed files" }));

  await fireEvent.click(screen.getByTitle("b.ts"));
  expect(await screen.findByText(/bravo_line/)).toBeInTheDocument();
  await back();
  await fireEvent.click(screen.getByTitle("a.ts"));
  await back();
  // Back to b.ts, served from the cache while a.ts is still in flight.
  await fireEvent.click(screen.getByTitle("b.ts"));
  expect(await screen.findByText(/bravo_line/)).toBeInTheDocument();
  expect(gitFileDiff.mock.calls.map((call) => call[2])).toEqual(["b.ts", "a.ts"]);

  slowA.resolve(patch("a.ts", "alpha_line"));
  await settle();
  expect(screen.queryByText(/alpha_line/)).not.toBeInTheDocument();
  expect(screen.getByText(/bravo_line/)).toBeInTheDocument();
});
