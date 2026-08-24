import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import Sidebar from "$lib/layout/Sidebar.svelte";
import { setProjectExpanded } from "$lib/services/api";
import { activeTurns, approvals, unansweredQuestions, userInputRequests } from "$lib/services/codexEvents.svelte";
import type { Project, SideQuestion, ThreadSection } from "$lib/types";
import { gitStatusCache } from "$lib/worktrees/gitStatus.svelte";

vi.mock("$lib/services/api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("$lib/services/api")>()),
  setProjectExpanded: vi.fn(() => Promise.resolve()),
}));

function project(pinned = false, threadPinned = false): Project {
  return {
    name: "codex-custom",
    path: "/projects/codex-custom",
    kind: "folder",
    pinned,
    expanded: true,
    threads: [
      {
        id: "thread-1",
        cwd: "/projects/codex-custom",
        title: "First thread",
        updatedAt: Date.now() / 1000,
        status: "idle",
        pinned: threadPinned,
      },
    ],
  };
}

function projectWithThreads(count: number): Project {
  const base = project();
  base.threads = Array.from({ length: count }, (_, index) => ({
    ...base.threads[0],
    id: `thread-${index + 1}`,
    title: `Thread ${index + 1}`,
  }));
  return base;
}

function setup(
  source = project(),
  sideQuestions: SideQuestion[] = [],
  selectedThread: string | null = null,
  sectionProps: { sections?: ThreadSection[]; sectionsSupported?: boolean } = {},
) {
  const onSelectThread = vi.fn();
  const onMenuAction = vi.fn();
  const onSelectArchived = vi.fn();
  const onNewThread = vi.fn();
  const result = render(Sidebar, {
    projects: [source],
    account: null,
    selectedThread,
    loading: false,
    sideQuestions,
    onAddProject: vi.fn(),
    onSelectThread,
    onNewThread,
    onOpenSettings: vi.fn(),
    onMenuAction,
    onSelectArchived,
    onUnarchived: vi.fn(),
    ...sectionProps,
  });
  return { ...result, onSelectThread, onMenuAction, onSelectArchived, onNewThread, source };
}

const SECTIONS: ThreadSection[] = [
  { id: "sec-week", name: "This week", color: "#f59e0b" },
  { id: "sec-later", name: "Later", color: null },
];

/** Three threads: one in each section and one unsectioned. */
function sectionedProject(): Project {
  const base = projectWithThreads(3);
  base.threads[0].sectionId = "sec-later";
  base.threads[1].sectionId = "sec-week";
  return base;
}

describe("Sidebar", () => {
  beforeEach(() => {
    activeTurns.list = [];
    approvals.list = [];
    userInputRequests.list = [];
    unansweredQuestions.list = [];
    gitStatusCache.byPath = {};
    gitStatusCache.loading = {};
    vi.mocked(setProjectExpanded).mockClear();
  });

  it("shows a working indicator for threads with an active turn", () => {
    activeTurns.list = ["thread-1"];
    setup();

    expect(screen.getByTitle("Working")).toBeInTheDocument();
  });

  it("shows a waiting indicator when a thread has a pending approval or question", () => {
    activeTurns.list = ["thread-1"];
    approvals.list = [{ requestId: 1, kind: "command", threadId: "thread-1", turnId: "t", itemId: "i" }];
    setup();

    expect(screen.getByTitle("Waiting for your input")).toBeInTheDocument();
    expect(screen.queryByTitle("Working")).not.toBeInTheDocument();
  });

  it("keeps waiting on a question stranded by an earlier session", () => {
    unansweredQuestions.list = ["thread-1"];
    setup();

    expect(screen.getByTitle("Waiting for your input")).toBeInTheDocument();
  });

  // Sections come from Codex ≥0.149; the sidebar groups each project's
  // threads under them in server order and keeps unsectioned threads last.
  it("groups threads under their sections in server order", () => {
    setup(sectionedProject(), [], null, { sections: SECTIONS, sectionsSupported: true });

    const rows = screen.getAllByText(/^(This week|Later|Thread \d)$/).map((node) => node.textContent);
    expect(rows).toEqual(["This week", "Thread 2", "Later", "Thread 1", "Thread 3"]);
  });

  it("offers section actions only on a Codex that has sections", async () => {
    const user = userEvent.setup();
    setup(sectionedProject(), [], null, { sections: SECTIONS, sectionsSupported: false });

    expect(screen.queryByText("This week")).not.toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: "Thread menu" })[0]);
    expect(screen.queryByRole("menuitem", { name: "Move to section" })).not.toBeInTheDocument();
  });

  it("reports section menu actions with the section as the target", async () => {
    const user = userEvent.setup();
    const { onMenuAction } = setup(sectionedProject(), [], null, { sections: SECTIONS, sectionsSupported: true });

    await user.click(screen.getAllByRole("button", { name: "Section menu" })[0]);
    await user.click(screen.getByRole("menuitem", { name: "Rename section" }));
    expect(onMenuAction).toHaveBeenCalledWith("rename", { kind: "section", section: SECTIONS[0] });

    await user.click(screen.getAllByRole("button", { name: "Thread menu" })[0]);
    await user.click(screen.getByRole("menuitem", { name: "Move to section" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("moveToSection", expect.objectContaining({ kind: "thread" }));
  });

  it("shows no activity indicator for idle threads", () => {
    setup();

    expect(screen.queryByTitle("Working")).not.toBeInTheDocument();
    expect(screen.queryByTitle("Waiting for your input")).not.toBeInTheDocument();
  });

  it("selects a thread with its owning project", async () => {
    const user = userEvent.setup();
    const { onSelectThread, source } = setup();

    await user.click(screen.getByRole("button", { name: /First thread/ }));

    expect(onSelectThread).toHaveBeenCalledWith(source, "thread-1");
  });

  it("collapses and expands a project", async () => {
    const user = userEvent.setup();
    setup();
    const projectButton = screen.getByText("codex-custom").closest("button") as HTMLButtonElement;

    expect(screen.getByText("First thread")).toBeVisible();
    await user.click(projectButton);
    expect(screen.getByText("First thread")).not.toBeVisible();
    await user.click(projectButton);
    expect(screen.getByText("First thread")).toBeVisible();
  });

  it("starts collapsed when the bootstrap project says it is closed", () => {
    const source = project();
    source.expanded = false;
    setup(source);

    expect(screen.getByText("First thread")).not.toBeVisible();
  });

  it("persists each project expansion change", async () => {
    const user = userEvent.setup();
    const { source } = setup();
    const projectButton = screen.getByText("codex-custom").closest("button") as HTMLButtonElement;

    await user.click(projectButton);

    expect(setProjectExpanded).toHaveBeenCalledWith(source.path, false);
  });

  it("exposes full project paths and thread names in tooltips", () => {
    const source = project();
    source.name = "codex-custom-with-a-long-project-name";
    source.threads[0].title = "A thread title that is longer than the sidebar can display";
    setup(source);

    expect(screen.getByLabelText(source.path)).toHaveTextContent(source.name);
    expect(screen.getByTitle(source.threads[0].title)).toHaveTextContent(source.threads[0].title);
  });

  it("shows the project path and branch as a multiline styled tooltip", async () => {
    const user = userEvent.setup();
    const source = project();
    gitStatusCache.byPath[source.path] = {
      branch: "feature/multiline-tooltip",
      detached: false,
      upstream: null,
      ahead: 0,
      behind: 0,
      counts: { staged: 0, unstaged: 0, untracked: 0, conflicted: 0 },
      files: [],
      truncated: false,
      refreshedAt: Date.now(),
    };
    gitStatusCache.fetchedAt[source.path] = Date.now();
    setup(source);

    const projectName = screen.getByText(source.name);
    await user.hover(projectName);

    const tooltip = await screen.findByRole("tooltip");
    expect(tooltip).toHaveClass("whitespace-pre-line");
    expect(tooltip.textContent).toBe(`${source.path}\n⎇ feature/multiline-tooltip`);
    await user.unhover(projectName);
  });

  it("shows a descendant subagent count on the root thread", () => {
    const source = project();
    source.threads[0].subagentCount = 3;
    setup(source);

    expect(screen.getByTitle("3 subagents")).toHaveTextContent("3");
  });

  it("routes rename and pin actions for the selected thread target", async () => {
    const user = userEvent.setup();
    const { onMenuAction, source } = setup();

    await user.click(screen.getByRole("button", { name: "Thread menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Rename thread" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("rename", {
      kind: "thread",
      project: source,
      thread: source.threads[0],
    });

    await user.click(screen.getByRole("button", { name: "Thread menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Pin thread" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("togglePin", {
      kind: "thread",
      project: source,
      thread: source.threads[0],
    });
  });

  it("offers unpin for pinned projects and threads", async () => {
    const user = userEvent.setup();
    setup(project(true, true));

    await user.click(screen.getByRole("button", { name: "Thread menu" }));
    expect(screen.getByRole("menuitem", { name: "Unpin thread" })).toBeVisible();
  });

  it("routes remove and reorder actions for projects", async () => {
    const user = userEvent.setup();
    const { onMenuAction, source } = setup();
    const target = { kind: "project", project: source };

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Move up" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("moveUp", target);

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Move down" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("moveDown", target);

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Remove project" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("remove", target);
  });

  it("hides remove and reorder for worktree projects", async () => {
    const user = userEvent.setup();
    const worktree = { ...project(), kind: "worktree" as const };
    setup(worktree);

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    expect(screen.queryByRole("menuitem", { name: "Remove project" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Move up" })).not.toBeInTheDocument();
  });

  it("treats multi-project workspaces as immutable containers while allowing thread moves", async () => {
    const user = userEvent.setup();
    const workspace = {
      ...project(),
      name: "API + Web",
      kind: "multiProject" as const,
      workspaceId: "workspace-1",
    };
    const { onMenuAction } = setup(workspace);

    await user.click(screen.getByRole("button", { name: "Project menu" }));
    expect(screen.getByRole("menuitem", { name: "Workspace details" })).toBeVisible();
    expect(screen.queryByRole("menuitem", { name: /Rename project/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /Pin project/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /Archive project/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Move up" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Thread menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Move to workspace" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("moveToWorkspace", {
      kind: "thread",
      project: workspace,
      thread: workspace.threads[0],
    });
  });

  it("starts a new thread in a project from its row button", async () => {
    const user = userEvent.setup();
    const { onNewThread, source } = setup();

    await user.click(screen.getByRole("button", { name: `New thread in ${source.name}` }));

    expect(onNewThread).toHaveBeenCalledWith(source);
  });

  it("routes the fork action for threads", async () => {
    const user = userEvent.setup();
    const { onMenuAction, source } = setup();

    await user.click(screen.getByRole("button", { name: "Thread menu" }));
    await user.click(screen.getByRole("menuitem", { name: "Fork thread" }));
    expect(onMenuAction).toHaveBeenLastCalledWith("fork", {
      kind: "thread",
      project: source,
      thread: source.threads[0],
    });
  });

  it("shows a side-question count badge on threads", () => {
    setup(project(), [
      { sideThreadId: "side-1", parentThreadId: "thread-1", title: "Why?", createdAt: 0 },
      { sideThreadId: "side-2", parentThreadId: "thread-1", title: "How?", createdAt: 1 },
    ]);

    expect(screen.getByTitle("2 side questions")).toBeInTheDocument();
  });

  it("truncates a project's thread list and reveals the rest on demand", async () => {
    const user = userEvent.setup();
    setup(projectWithThreads(20));

    expect(screen.getByText("Thread 15")).toBeInTheDocument();
    expect(screen.queryByText("Thread 16")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show 5 more" }));
    expect(screen.getByText("Thread 20")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show less" }));
    expect(screen.queryByText("Thread 16")).not.toBeInTheDocument();
  });

  it("does not truncate projects at or under the limit", () => {
    setup(projectWithThreads(15));

    expect(screen.getByText("Thread 15")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Show \d+ more/ })).not.toBeInTheDocument();
  });

  it("keeps the selected thread visible past the truncation point", () => {
    setup(projectWithThreads(20), [], "thread-19");

    expect(screen.getByText("Thread 19")).toBeInTheDocument();
    expect(screen.queryByText("Thread 18")).not.toBeInTheDocument();
  });

  it("lists archived threads and selects them", async () => {
    const user = userEvent.setup();
    const { onSelectArchived } = setup();

    await user.click(screen.getByRole("button", { name: /Archived/ }));
    await user.click(await screen.findByRole("button", { name: /^Old research thread/ }));
    expect(onSelectArchived).toHaveBeenCalledWith(
      expect.objectContaining({ id: "archived-1", title: "Old research thread" }),
    );
  });
});
