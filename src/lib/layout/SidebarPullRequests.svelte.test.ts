import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import Sidebar from "$lib/layout/Sidebar.svelte";
import { threadSummary } from "$lib/services/preview/factories";
import type { Project } from "$lib/types";

vi.mock("$lib/services/homeRouting", async (importOriginal) => {
  const actual = await importOriginal<typeof import("$lib/services/homeRouting")>();
  return { ...actual, commands: { ...actual.commands, setProjectExpanded: vi.fn(() => Promise.resolve(null)) } };
});

function project(name: string): Project {
  const path = `/projects/${name}`;
  return {
    name,
    path,
    kind: "folder",
    workspaceId: null,
    archived: false,
    instructions: "",
    sources: [],
    pinned: false,
    expanded: false,
    threads: [threadSummary({ id: `${name}-1`, title: "A thread", cwd: path })],
  };
}

function setup(needsYou: Record<string, number>) {
  const onOpenPullRequests = vi.fn();
  const projects = $state([project("pingex"), project("notes")]);
  render(Sidebar, {
    projects,
    account: null,
    selectedThread: null,
    loading: false,
    onAddProject: vi.fn(),
    onSelectThread: vi.fn(),
    onNewThread: vi.fn(),
    onOpenSettings: vi.fn(),
    onMenuAction: vi.fn(),
    onOpenPullRequests,
    needsYou,
  });
  return { onOpenPullRequests, user: userEvent.setup() };
}

describe("Sidebar pull requests", () => {
  it("offers Pull requests in the top section with how many need me in all", async () => {
    const { onOpenPullRequests, user } = setup({ "/projects/pingex": 3, "/projects/notes": 0 });
    const destination = screen.getByRole("button", { name: /Pull requests/ });
    expect(destination).toHaveTextContent("3");
    await user.click(destination);
    expect(onOpenPullRequests).toHaveBeenCalledWith();
  });

  it("badges each repository with how many need me, and opens its inbox", async () => {
    const { onOpenPullRequests, user } = setup({ "/projects/pingex": 3, "/projects/notes": 0 });
    expect(screen.queryByRole("button", { name: /0 need you/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "3 need you" }));
    expect(onOpenPullRequests).toHaveBeenCalledWith("/projects/pingex");
  });
});
