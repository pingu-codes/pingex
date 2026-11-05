import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Sidebar from "$lib/layout/Sidebar.svelte";
import { resetRepositories } from "$lib/repositories/repositoryStore.svelte";
import { commands } from "$lib/services/homeRouting";
import { resetPreviewRepositories } from "$lib/services/preview/repositories";
import type { Project } from "$lib/types";

// The sidebar's repository states over the preview fakes: codex-custom is
// cloned, notes has no GitHub remote, and openai/codex is PRs only.

function project(name: string, path: string): Project {
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
    threads: [],
  };
}

function setup() {
  const onAddRepository = vi.fn();
  const projects = $state([
    project("codex-custom", "/Users/ciaran/Projects/codex-custom"),
    project("notes", "/Users/ciaran/Projects/notes"),
  ]);
  render(Sidebar, {
    projects,
    account: null,
    selectedThread: null,
    loading: false,
    sideQuestions: [],
    onAddProject: vi.fn(),
    onAddRepository,
    onSelectThread: vi.fn(),
    onNewThread: vi.fn(),
    onOpenSettings: vi.fn(),
    onMenuAction: vi.fn(),
  });
  return { onAddRepository, user: userEvent.setup() };
}

describe("Sidebar repository states", () => {
  beforeEach(() => {
    resetPreviewRepositories();
    resetRepositories();
  });

  it("marks a project with no GitHub remote and shows nothing extra on a cloned one", async () => {
    setup();

    const notes = await screen.findByTestId("repository-mark:/Users/ciaran/Projects/notes");
    expect(notes).toHaveTextContent("no PRs");
    expect(screen.queryByTestId("repository-mark:/Users/ciaran/Projects/codex-custom")).not.toBeInTheDocument();
  });

  it("lists PRs-only repositories with a cloud and offers Clone and Link existing folder", async () => {
    const { onAddRepository, user } = setup();

    const group = await screen.findByRole("group", { name: "PRs only" });
    const row = within(group).getByText("codex").closest("[data-repository]") as HTMLElement;
    expect(within(row).getByLabelText("PRs only, not cloned")).toBeInTheDocument();

    await user.click(within(row).getByRole("button", { name: "Repository menu" }));
    await user.click(screen.getByRole("menuitem", { name: /Clone…/ }));
    expect(onAddRepository).toHaveBeenLastCalledWith({ source: "url", url: "openai/codex", urlMode: "clone" });

    await user.click(within(row).getByRole("button", { name: "Repository menu" }));
    await user.click(screen.getByRole("menuitem", { name: /Link existing folder…/ }));
    expect(onAddRepository).toHaveBeenLastCalledWith({ source: "folder", linkSlug: "openai/codex" });
  });

  it("removes a PRs-only repository from its menu", async () => {
    const { user } = setup();
    const group = await screen.findByRole("group", { name: "PRs only" });

    await user.click(within(group).getByRole("button", { name: "Repository menu" }));
    await user.click(screen.getByRole("menuitem", { name: /Remove/ }));

    await waitFor(() => expect(screen.queryByRole("group", { name: "PRs only" })).not.toBeInTheDocument());
    expect(await commands.reviewPrsOnlyRepositories()).toEqual([]);
  });

  it("opens Add repository from the Projects heading", async () => {
    const { onAddRepository, user } = setup();

    await user.click(screen.getByRole("button", { name: "Add repository" }));

    expect(onAddRepository).toHaveBeenCalledWith();
  });
});
