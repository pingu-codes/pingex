<script lang="ts">
  import {
    Bot,
    Brain,
    ChevronDown,
    Ellipsis,
    EyeOff,
    Folder,
    FolderGit2,
    FolderOpen,
    FolderPlus,
    FolderTree,
    GitBranch,
    GitPullRequest,
    Layers3,
    ListRestart,
    MessageCircleQuestion,
    Pin,
    Plus,
    Search,
    Settings,
    Smartphone,
    SquarePen,
    Star,
    Zap,
  } from "@lucide/svelte";
  import { Collapsible, useMenu } from "@skeletonlabs/skeleton-svelte";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import ArchivedThreadsSection from "$lib/layout/ArchivedThreadsSection.svelte";
  import { activeConnectionCount, hasActiveConnection } from "$lib/layout/connectionState";
  import SidebarContextMenu from "$lib/layout/SidebarContextMenu.svelte";
  import SidebarSearch from "$lib/layout/SidebarSearch.svelte";
  import { type DragHooks, dnd, draggable, rowId, rowRef } from "$lib/layout/sidebarDnd.svelte";
  import { isStale, sidebarPrefs } from "$lib/layout/sidebarPrefs.svelte";
  import {
    buildTree,
    type DropTarget,
    emptyLayout,
    flattenItems,
    isNoopDrop,
    ROOT_SCOPE,
    resolveDrop,
    siblingsAfterDrop,
    type TreeNode,
  } from "$lib/layout/sidebarTree";
  import UsageMeter from "$lib/layout/UsageMeter.svelte";
  import type { AddRepositoryInitial } from "$lib/repositories/AddRepositoryDialog.svelte";
  import RepositoryMark from "$lib/repositories/RepositoryMark.svelte";
  import { ensureRepositories } from "$lib/repositories/repositoryStore.svelte";
  import SidebarRepositories from "$lib/repositories/SidebarRepositories.svelte";
  import { STEP_LABELS } from "$lib/review/actions";
  import { ensurePrLinks, prLinks } from "$lib/review/prLinks.svelte";
  import { fileReviewThreads, withoutTaken } from "$lib/review/reviewFiling";
  import StepDot from "$lib/review/StepDot.svelte";
  import { accountUsage } from "$lib/services/accountUsage.svelte";
  import { activeTurns, approvals, unansweredQuestions, userInputRequests } from "$lib/services/codexEvents.svelte";
  import { remoteConnections } from "$lib/services/connections.svelte";
  import { commands, projectHostLabel } from "$lib/services/homeRouting";
  import { toastError } from "$lib/toaster";
  import type {
    Account,
    ArchivedThread,
    BootstrapData,
    MenuAction,
    MenuTarget,
    Project,
    SidebarFolder,
    SidebarItemRef,
    SidebarLayout,
    SideQuestion,
    ThreadSearchItem,
    ThreadSection,
    ThreadSummary,
  } from "$lib/types";
  import { harnessName } from "$lib/utils/capabilities";
  import { dragRegion } from "$lib/utils/dragRegion";
  import { errorMessage } from "$lib/utils/errors";
  import { relativeTime } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";
  import BranchChip from "$lib/worktrees/BranchChip.svelte";
  import { ensureGitStatus, gitStatusCache } from "$lib/worktrees/gitStatus.svelte";
  import { ensureWorktrees, worktreeMap } from "$lib/worktrees/worktreeMap.svelte";
  import { nestProjects, type WorktreeChild } from "$lib/worktrees/worktreeTree";

  let {
    projects,
    account,
    selectedThread,
    loading,
    sideQuestions = [],
    sections = [],
    sectionsSupported = false,
    onAddProject,
    onAddRepository,
    onAddWorkspace,
    onSelectThread,
    onNewThread,
    onGoHome,
    onOpenSettings,
    onOpenMemories,
    memoriesOpen = false,
    onMenuAction,
    onSelectArchived,
    onUnarchived,
    onOpenWorktrees,
    currentProject = null,
    sidebarLayout = emptyLayout(),
    onNewFolder,
    onMoveItem,
    onOpenSearchResult,
    onSessionFocus,
    onResetOrder,
    unfiledThreads = [],
    onNewThreadInDir,
    onAddWorktreeProject,
    onOpenPullRequests,
    pullRequestsOpen = false,
    needsYou = {},
  }: {
    projects: Project[];
    account: Account | null;
    selectedThread: string | null;
    loading: boolean;
    sideQuestions?: SideQuestion[];
    /** Server thread sections (Codex ≥0.149), in server order. */
    sections?: ThreadSection[];
    /** False hides every section affordance (older Codex). */
    sectionsSupported?: boolean;
    onAddProject: () => void;
    /** Opens Add repository; omitted, the sidebar shows no repository UI. */
    onAddRepository?: (initial?: AddRepositoryInitial) => void;
    onAddWorkspace?: () => void;
    onSelectThread: (project: Project, threadId: string) => void;
    onNewThread: (project?: Project) => void;
    onGoHome?: () => void;
    onOpenSettings: () => void;
    /** Opens the Memories page; the button is hidden without it. */
    onOpenMemories?: () => void;
    /** Whether the Memories page is the one showing. */
    memoriesOpen?: boolean;
    onMenuAction: (action: MenuAction, target: MenuTarget) => void;
    onSelectArchived?: (thread: ArchivedThread) => void;
    onUnarchived?: (data: BootstrapData) => void;
    onOpenWorktrees?: (project: Project) => void;
    currentProject?: Project | null;
    onOpenSearchResult?: (item: ThreadSearchItem) => void;
    /** User-made folders and drag orderings; the tree is built from these. */
    sidebarLayout?: SidebarLayout;
    onNewFolder?: () => void;
    /** A drag landed: `item` now sits under `parentId` with `siblings` as the full order. */
    onMoveItem?: (
      scope: string,
      item: SidebarItemRef,
      parentId: string | null,
      siblings: SidebarItemRef[],
    ) => void | Promise<void>;
    /** The focus button: hide untouched threads and collapse what is left empty. */
    onSessionFocus?: () => void;
    /** Forget drag ordering in `scope` (`ROOT_SCOPE` for projects). */
    onResetOrder?: (scope: string) => void;
    /** Threads no project claims; nested mode files them under unadded worktrees. */
    unfiledThreads?: ThreadSummary[];
    /** Start a draft in an exact directory (an unadded worktree). */
    onNewThreadInDir?: (cwd: string) => void;
    /** The Pull requests destination, or one repository's inbox; hidden without it. */
    onOpenPullRequests?: (path?: string) => void;
    pullRequestsOpen?: boolean;
    /** PRs that need me, by repository folder, from the last inbox sync. */
    needsYou?: Record<string, number>;
    /** Add an unadded worktree to the sidebar as a project. */
    onAddWorktreeProject?: (path: string) => void;
  } = $props();

  // --- PR links -----------------------------------------------------------------
  // Threads about someone else's PR, or my PR without a worktree, leave their
  // cwd's list for their repository's Reviews group (`reviewFiling.ts`).
  $effect(() => {
    void ensurePrLinks();
  });
  const filing = $derived(
    fileReviewThreads(
      projects.filter((project) => !project.archived),
      unfiledThreads,
      prLinks.byThread,
    ),
  );
  const reviewThreads = (project: Project) => filing.reviews[project.path] ?? [];
  const reviewsScope = (path: string) => `reviews:${path}`;

  const visibleProjects = $derived(
    projects.filter((project) => !project.archived).map((project) => withoutTaken(project, filing.taken)),
  );

  // --- Worktree nesting --------------------------------------------------------
  // With nesting on, each repository row carries its worktrees: adopted
  // worktree projects move under it, and worktrees nobody added appear as
  // child rows holding the threads whose cwd is inside them.
  const nesting = $derived(sidebarPrefs.nestWorktrees);
  const repoPaths = $derived(
    visibleProjects.filter((project) => project.kind === "folder").map((project) => project.path),
  );
  $effect(() => {
    if (nesting) ensureWorktrees(repoPaths);
  });
  // Each repository's state: cloned, PRs only or no GitHub remote.
  $effect(() => {
    if (onAddRepository) void ensureRepositories(repoPaths);
  });
  const nested = $derived(
    nesting
      ? nestProjects(
          visibleProjects,
          worktreeMap.byRepo,
          unfiledThreads.filter((thread) => !filing.taken.has(thread.id)),
        )
      : null,
  );
  $effect(() => {
    worktreeMap.unaddedParents = nested?.unaddedParents ?? {};
  });
  /** Projects drawn at the top level. */
  const rootProjects = $derived(nested?.roots ?? visibleProjects);
  /** Every project drawn with a row of its own, nested or not. */
  const rowProjects = $derived(
    nested
      ? [
          ...nested.roots,
          ...Object.values(nested.children)
            .flat()
            .flatMap((child) => (child.adopted ? [child.adopted] : [])),
        ]
      : visibleProjects,
  );
  const worktreeChildren = (project: Project): WorktreeChild[] => nested?.children[project.path] ?? [];
  /** Unadded worktree rows the user opened or closed, by path. */
  let unaddedOpen = $state<Record<string, boolean>>({});
  /** Drag scopes no tree answers to, so nested rows stay where nesting puts them. */
  const nestedScope = (path: string) => `nested:${path}`;

  interface HeaderAction {
    label: string;
    icon: typeof Plus;
    onclick: () => void;
    testId?: string;
    /** A toggle that is on: drawn pressed and kept visible. */
    active?: boolean;
  }

  // The Projects heading's hover-revealed buttons; optional ones show only when
  // the parent wires them.
  const headerActions = $derived(
    (
      [
        onSessionFocus && {
          label: "Focus: hide threads not opened this session or favorited",
          icon: Zap,
          onclick: onSessionFocus,
          testId: "session-focus",
        },
        onResetOrder && { label: "Reset project order", icon: ListRestart, onclick: () => onResetOrder(ROOT_SCOPE) },
        onAddWorkspace && { label: "Create multi-project workspace", icon: Layers3, onclick: onAddWorkspace },
        onNewFolder && { label: "New folder", icon: FolderPlus, onclick: onNewFolder },
        {
          label: nesting ? "Show worktrees as separate projects" : "Nest worktrees under their repository",
          icon: FolderTree,
          onclick: () => sidebarPrefs.setNestWorktrees(!nesting),
          testId: "nest-worktrees",
          active: nesting,
        },
        onAddRepository && { label: "Add repository", icon: GitPullRequest, onclick: () => onAddRepository() },
        { label: "Add project folder", icon: Plus, onclick: onAddProject },
      ] as (HeaderAction | false | undefined)[]
    ).filter((action): action is HeaderAction => Boolean(action)),
  );

  // One busy project shouldn't push every other project off-screen, so expanded
  // projects show a head slice until the user asks for the rest. Threads the
  // user hid and, with the "hide old threads" preference on, day-old threads
  // fold behind the same button; favorited and selected threads always stay in
  // view.
  const THREAD_LIMIT = 15;
  /** "Show more" reveals this many folded threads per click; Cmd/Ctrl-click reveals all. */
  const REVEAL_STEP = 10;
  /** Folded threads revealed past the head, per project path. */
  let revealed = $state<Record<string, number>>({});
  /** True while Cmd/Ctrl is held, so "Show more" can advertise the reveal-all count. */
  let modifierHeld = $state(false);

  function splitThreads(project: Project): { head: ThreadSummary[]; hidden: number } {
    const keep = (thread: ThreadSummary) =>
      thread.id === selectedThread ||
      thread.pinned ||
      (!thread.hidden && !(sidebarPrefs.hideOldThreads && isStale(thread.updatedAt)));
    const head = project.threads.filter(keep).slice(0, THREAD_LIMIT);
    // Selection can come from outside the sidebar; keep it visible even when it
    // sorts past the cap.
    if (selectedThread && !head.some((thread) => thread.id === selectedThread)) {
      const selected = project.threads.find((thread) => thread.id === selectedThread);
      if (selected) head.push(selected);
    }
    return { head, hidden: project.threads.length - head.length };
  }

  function visibleThreads(project: Project) {
    const { head } = splitThreads(project);
    const extra = revealed[project.path] ?? 0;
    if (extra <= 0) return head;
    const shown = new Set(head.map((thread) => thread.id));
    for (const thread of project.threads) {
      if (shown.size >= head.length + extra) break;
      shown.add(thread.id);
    }
    // Draw in project order so folder placements and "hide below" stay stable.
    return project.threads.filter((thread) => shown.has(thread.id));
  }

  function revealMore(project: Project, all: boolean) {
    const { hidden } = splitThreads(project);
    const current = revealed[project.path] ?? 0;
    revealed[project.path] = all ? hidden : Math.min(hidden, current + REVEAL_STEP);
  }

  /** The visible threads split by section: each section that has a thread in
   *  this project (in server order), then the unsectioned rest. A section the
   *  server no longer lists is treated as no section. */
  function threadGroups(visible: ThreadSummary[]): { section: ThreadSection | null; threads: ThreadSummary[] }[] {
    if (!sectionsSupported || sections.length === 0) return [{ section: null, threads: visible }];
    const groups: { section: ThreadSection | null; threads: ThreadSummary[] }[] = sections
      .map((section) => ({ section, threads: visible.filter((thread) => thread.sectionId === section.id) }))
      .filter((group) => group.threads.length > 0);
    const known = new Set(sections.map((section) => section.id));
    const rest = visible.filter((thread) => !thread.sectionId || !known.has(thread.sectionId));
    if (rest.length > 0 || groups.length === 0) groups.push({ section: null, threads: rest });
    return groups;
  }

  // --- Folder tree & drag-and-drop -------------------------------------------

  const rootAdapter = {
    key: (project: Project) => project.path,
    pinned: (project: Project) => project.pinned,
    recency: (project: Project) => project.recencyAt,
  };
  const threadAdapter = { key: (thread: ThreadSummary) => thread.id, pinned: (thread: ThreadSummary) => thread.pinned };

  const rootTree = $derived(buildTree(sidebarLayout, ROOT_SCOPE, rootProjects, rootAdapter));
  const projectTree = (project: Project) =>
    buildTree(sidebarLayout, project.path, visibleThreads(project), threadAdapter);

  /** Every thread of `project` in the order the sidebar draws them, "Show more"
   *  or not: folders first-to-last, each run split by section like `threadList`. */
  function orderedThreadIds(project: Project): string[] {
    const walk = (nodes: TreeNode<ThreadSummary>[]): string[] =>
      threadRuns(nodes).flatMap((run) =>
        run.kind === "folder"
          ? walk(run.node.children)
          : threadGroups(run.threads).flatMap((group) => group.threads.map((thread) => thread.id)),
      );
    return walk(buildTree(sidebarLayout, project.path, project.threads, threadAdapter));
  }

  function treeFor(scope: string): TreeNode<Project>[] | TreeNode<ThreadSummary>[] | null {
    if (scope === ROOT_SCOPE) return rootTree;
    const project = rowProjects.find((candidate) => candidate.path === scope);
    return project ? projectTree(project) : null;
  }

  /** Consecutive item nodes are rendered as one run so section headers can
   *  split them; folders break the runs. */
  type ThreadRun =
    | { kind: "folder"; node: Extract<TreeNode<ThreadSummary>, { kind: "folder" }> }
    | { kind: "threads"; threads: ThreadSummary[] };
  function threadRuns(nodes: TreeNode<ThreadSummary>[]): ThreadRun[] {
    const runs: ThreadRun[] = [];
    for (const node of nodes) {
      if (node.kind === "folder") runs.push({ kind: "folder", node });
      else {
        const last = runs[runs.length - 1];
        if (last?.kind === "threads") last.threads.push(node.item);
        else runs.push({ kind: "threads", threads: [node.item] });
      }
    }
    return runs;
  }

  const dragHooks: DragHooks = {
    resolve(row, pointerY) {
      const dragging = dnd.dragging;
      const target = rowRef(row);
      if (!dragging || !target || target.scope !== dragging.scope) return null;
      const tree = treeFor(dragging.scope);
      if (!tree) return null;
      // Both trees share the same node shape; the item type is irrelevant here.
      const drop = resolveDrop(
        tree as TreeNode<unknown>[],
        sidebarLayout,
        dragging.ref,
        target.ref,
        pointerY,
        row.getBoundingClientRect(),
      );
      return drop && !isNoopDrop(tree as TreeNode<unknown>[], dragging.ref, drop) ? drop : null;
    },
    commit(target) {
      const dragging = dnd.dragging ?? lastDrag;
      const tree = dragging ? treeFor(dragging.scope) : null;
      if (!dragging || !tree) return;
      const siblings = siblingsAfterDrop(tree as TreeNode<unknown>[], dragging.ref, target);
      return onMoveItem?.(dragging.scope, dragging.ref, target.parentId, siblings);
    },
  };
  // `commit` runs after the drag state is cleared, so remember what was dragged.
  let lastDrag: typeof dnd.dragging = null;
  $effect(() => {
    if (dnd.dragging) lastDrag = dnd.dragging;
  });

  const dragSource = (scope: string, ref: SidebarItemRef, label: string) => ({
    source: { scope, ref, label },
    hooks: dragHooks,
  });
  const folderRef = (folder: SidebarFolder): SidebarItemRef => ({ kind: "folder", id: folder.id });
  const dropClass = (ref: SidebarItemRef) => (dnd.over?.rowId === rowId(ref) ? `drop-${dnd.over.zone}` : "");

  function persistFolderExpansion(folder: SidebarFolder, expanded: boolean) {
    folder.expanded = expanded;
    void commands.setSidebarFolderExpanded(folder.id, expanded).catch((cause) => {
      toastError(`Could not save folder expansion: ${errorMessage(cause)}`);
    });
  }
  const folderTarget = (folder: SidebarFolder, project: Project | null): MenuTarget => ({
    kind: "folder",
    folder,
    project,
  });

  let searching = $state(false);

  function openSearch() {
    searching = true;
  }

  function trackModifier(event: KeyboardEvent) {
    modifierHeld = event.metaKey || event.ctrlKey;
  }

  function onWindowKeydown(event: KeyboardEvent) {
    trackModifier(event);
    // Cmd/Ctrl+Shift+F opens sidebar search (Shift avoids clashing with the
    // in-thread find shortcut).
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "f") {
      event.preventDefault();
      searching = true;
    }
  }

  // "waiting" wins over "working": a blocked turn still counts as inProgress,
  // but the user action needed is the more useful signal. Questions stranded by
  // an earlier session count too — the sidebar is the only way to find them.
  const threadActivity = (threadId: string): "waiting" | "working" | null => {
    if (
      approvals.list.some((approval) => approval.threadId === threadId) ||
      userInputRequests.list.some((request) => request.threadId === threadId) ||
      unansweredQuestions.list.includes(threadId)
    ) {
      return "waiting";
    }
    return activeTurns.list.includes(threadId) ? "working" : null;
  };

  const sideQuestionCount = (threadId: string) =>
    sideQuestions.filter((entry) => entry.parentThreadId === threadId).length;

  // Two row menus share one target and one item list: rows are context
  // triggers of one, ellipsis buttons triggers of the other. Zag only anchors a
  // menu that has context triggers at the pointer, so the button menu is kept
  // separate to anchor below its button. Triggers are keyed by `menuKey`, and
  // whichever fired last sets the target the items are built from.
  let menuTarget = $state<MenuTarget | null>(null);
  const menuId = $props.id();
  const rowMenuProps = (id: string) => ({
    id,
    onSelect: ({ value }: { value: string }) => {
      if (menuTarget) onMenuAction(value as MenuAction, menuTarget);
    },
    positioning: { placement: "bottom-start" as const, gutter: 4 },
  });
  const contextMenu = useMenu(rowMenuProps(`${menuId}-context`));
  const buttonMenu = useMenu(rowMenuProps(`${menuId}-button`));

  function menuKey(target: MenuTarget): string {
    switch (target.kind) {
      case "project":
        return `project:${target.project.path}`;
      case "thread":
        return `thread:${target.thread.id}`;
      case "section":
        return `section:${target.section.id}`;
      case "folder":
        return `folder:${target.folder.id}`;
    }
  }

  type Handler = ((event: Event) => void) | null | undefined;

  /** Zag's trigger props with `target` recorded before each opening event reaches the menu. */
  function aimedAt<Props extends object>(props: Props, target: MenuTarget, events: string[]): Props {
    const aimed = { ...props } as Record<string, unknown>;
    for (const name of events) {
      const original = (props as Record<string, unknown>)[name] as Handler;
      aimed[name] = (event: Event) => {
        menuTarget = target;
        original?.(event);
      };
    }
    return aimed as Props;
  }

  const contextMenuTrigger = (target: MenuTarget) =>
    aimedAt(contextMenu().getContextTriggerProps({ value: menuKey(target) }), target, [
      "oncontextmenu",
      "onpointerdown",
    ]);

  const menuButton = (target: MenuTarget) =>
    aimedAt(buttonMenu().getTriggerProps({ value: menuKey(target) }), target, ["onclick", "onkeydown"]);

  const connectedDevices = $derived(hasActiveConnection(remoteConnections.list));
  const onlineCount = $derived(activeConnectionCount(remoteConnections.list));

  const projectTarget = (project: Project): MenuTarget => ({ kind: "project", project });

  function persistProjectExpansion(project: Project, expanded: boolean) {
    project.expanded = expanded;
    // Nesting may hand the row a copy; the listed project is what persists.
    const listed = projects.find((candidate) => candidate.path === project.path);
    if (listed) listed.expanded = expanded;
    void commands.setProjectExpanded(project.path, expanded).catch((cause) => {
      toastError(`Could not save project expansion: ${errorMessage(cause)}`);
    });
  }

  /** Tooltip body for a project row: full path plus the checked-out branch once known. */
  function projectTooltip(project: Project): string {
    const branch = gitStatusCache.byPath[project.path]?.branch;
    return branch ? `${project.path}\n⎇ ${branch}` : project.path;
  }
  const threadTarget = (project: Project, thread: ThreadSummary): MenuTarget => ({
    kind: "thread",
    project,
    thread,
    order: orderedThreadIds(project),
  });
  const sectionTarget = (section: ThreadSection): MenuTarget => ({ kind: "section", section });
</script>

<svelte:window onkeydown={onWindowKeydown} onkeyup={trackModifier} onblur={() => (modifierHeld = false)} />

{#snippet threadRow(project: Project, thread: ThreadSummary, scope: string = project.path)}
  <div
    class="group/thread relative rounded-md {dropClass({ kind: "item", id: thread.id })} {thread.hidden
      ? "opacity-50"
      : ""}"
    role="presentation"
    {...contextMenuTrigger(threadTarget(project, thread))}
    use:draggable={dragSource(scope, { kind: "item", id: thread.id }, thread.title)}
  >
    <button
      onclick={() => onSelectThread(project, thread.id)}
      class="flex w-full items-center gap-2 rounded-md py-1.5 pl-2 pr-7 text-left text-[12px] transition {selectedThread ===
      thread.id
        ? "preset-tonal"
        : "text-surface-700-300 hover:preset-tonal"}"
    >
      {#if threadActivity(thread.id) === "waiting"}
        <span class="relative grid size-2 shrink-0 place-items-center" role="status" title="Waiting for your input">
          <span class="absolute size-2 animate-ping rounded-full bg-warning-500/60"></span>
          <span class="size-2 rounded-full bg-warning-500"></span>
        </span>
      {:else if threadActivity(thread.id) === "working"}
        <span class="grid size-2 shrink-0 place-items-center" role="status" title="Working">
          <span class="size-2 animate-pulse rounded-full bg-primary-500"></span>
        </span>
      {/if}
      {#if thread.pinned}
        <Star class="shrink-0 fill-warning-500 text-warning-500" size={11} />
      {/if}
      {#if thread.hidden}
        <span class="grid shrink-0 place-items-center text-surface-500" title="Hidden">
          <EyeOff size={11} />
        </span>
      {/if}
      <span class="min-w-0 flex-1 truncate" title={thread.title}>{thread.title}</span>
      {#if thread.harness}
        <!-- Only threads off the default harness carry one, so name it. -->
        <span
          class="shrink-0 rounded px-1 text-[9px] font-semibold uppercase tracking-wide text-surface-500 preset-tonal"
          title="Runs on {harnessName(thread.harness)}"
        >
          {harnessName(thread.harness)}
        </span>
      {/if}
      {#if thread.inTempWorktree}
        <span
          class="grid shrink-0 place-items-center text-surface-500"
          title="Runs in the temporary worktree {basename(thread.cwd ?? "")}"
        >
          <GitBranch size={10} />
        </span>
      {/if}
      {#if prLinks.byThread[thread.id]}
        {@const link = prLinks.byThread[thread.id]}
        <span
          class="inline-flex shrink-0 items-center gap-1 rounded px-1 font-mono text-[10px] text-surface-500 preset-tonal"
          title="#{link.prNumber} · started at {STEP_LABELS[link.step].toLowerCase()}"
          data-testid="pr-chip"
        >
          <StepDot step={link.step} />#{link.prNumber}
        </span>
      {/if}
      {#if sideQuestionCount(thread.id) > 0}
        <span
          class="inline-flex shrink-0 items-center gap-0.5 rounded-full bg-primary-500/15 px-1.5 text-[9px] font-medium text-primary-500"
          title="{sideQuestionCount(thread.id)} side question{sideQuestionCount(thread.id) === 1 ? "" : "s"}"
        >
          <MessageCircleQuestion size={9} />
          {sideQuestionCount(thread.id)}
        </span>
      {/if}
      {#if (thread.subagentCount ?? 0) > 0}
        <span
          class="inline-flex shrink-0 items-center gap-0.5 rounded-full bg-success-500/15 px-1.5 text-[9px] font-medium text-success-600 dark:text-success-400"
          title="{thread.subagentCount} subagent{thread.subagentCount === 1 ? "" : "s"}"
        >
          <Bot size={9} />
          {thread.subagentCount}
        </span>
      {/if}
      <span class="shrink-0 text-[10px] text-surface-500 group-hover/thread:opacity-0"
        >{relativeTime(thread.updatedAt)}</span
      >
    </button>
    <button
      type="button"
      aria-label="Thread menu"
      {...menuButton(threadTarget(project, thread))}
      class="absolute right-0.5 top-1/2 size-6 -translate-y-1/2 row-action group-hover/thread:opacity-100"
    >
      <Ellipsis size={13} />
    </button>
  </div>
{/snippet}

{#snippet folderNode(
  folder: SidebarFolder,
  project: Project | null,
  children: TreeNode<ThreadSummary>[] | TreeNode<Project>[],
)}
  <Collapsible
    class="min-w-0 items-stretch"
    open={folder.expanded}
    onOpenChange={({ open }) => persistFolderExpansion(folder, open)}
  >
    <div
      class="group/folder relative flex items-center rounded-md {dropClass(folderRef(folder))}"
      role="presentation"
      {...contextMenuTrigger(folderTarget(folder, project))}
      use:draggable={dragSource(project?.path ?? ROOT_SCOPE, folderRef(folder), folder.name)}
    >
      <Collapsible.Trigger
        class="group flex min-w-0 flex-1 items-center gap-2 rounded-md py-1.5 pl-2 pr-7 text-left text-[13px] font-medium group-hover/folder:preset-tonal"
      >
        <ChevronDown class="text-surface-500 transition group-data-[state=closed]:-rotate-90" size={14} />
        <FolderOpen class="text-warning-500 group-data-[state=closed]:hidden" size={15} strokeWidth={1.7} />
        <Folder class="hidden text-warning-500 group-data-[state=closed]:block" size={15} strokeWidth={1.7} />
        <span class="min-w-0 flex-1 truncate" title={folder.name}>{folder.name}</span>
        <span class="text-[10px] font-normal text-surface-500 group-hover/folder:opacity-0"
          >{flattenItems<unknown>(children).length}</span
        >
      </Collapsible.Trigger>
      <button
        type="button"
        aria-label="Folder menu"
        {...menuButton(folderTarget(folder, project))}
        class="absolute right-1 top-1/2 size-6 -translate-y-1/2 row-action group-hover/folder:opacity-100"
      >
        <Ellipsis size={14} />
      </button>
    </div>
    <Collapsible.Content class="ml-[27px] border-l border-surface-200-800 pl-1.5">
      {#if children.length === 0}
        <p class="px-2 py-1.5 text-xs text-surface-500">Drop {project ? "threads" : "projects"} here</p>
      {:else if project}
        {@render threadNodes(project, children as TreeNode<ThreadSummary>[])}
      {:else}
        {@const projectNodes = children as TreeNode<Project>[]}
        {#each projectNodes as child (rowId(child))}
          {@render rootNode(child)}
        {/each}
      {/if}
    </Collapsible.Content>
  </Collapsible>
{/snippet}

{#snippet threadList(project: Project, threads: ThreadSummary[])}
  {#each threadGroups(threads) as group (group.section?.id ?? "")}
    {#if group.section}
      <div
        class="group/section relative mt-1 flex items-center"
        role="presentation"
        {...contextMenuTrigger(sectionTarget(group.section!))}
      >
        <div
          class="flex min-w-0 flex-1 items-center gap-1.5 px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.06em] text-surface-500"
        >
          <span
            class="size-1.5 shrink-0 rounded-full"
            style="background: {group.section.color ?? "currentColor"}"
          ></span>
          <span class="truncate">{group.section.name}</span>
        </div>
        <button
          type="button"
          aria-label="Section menu"
          {...menuButton(sectionTarget(group.section!))}
          class="size-5 shrink-0 row-action group-hover/section:opacity-100"
        >
          <Ellipsis size={12} />
        </button>
      </div>
    {/if}
    {#each group.threads as thread (thread.id)}
      {@render threadRow(project, thread)}
    {/each}
  {/each}
{/snippet}

{#snippet threadNodes(project: Project, nodes: TreeNode<ThreadSummary>[])}
  {#each threadRuns(nodes) as run, index (run.kind === "folder" ? rowId(folderRef(run.node.folder)) : `run-${index}`)}
    {#if run.kind === "folder"}
      {@render folderNode(run.node.folder, project, run.node.children)}
    {:else}
      {@render threadList(project, run.threads)}
    {/if}
  {/each}
{/snippet}

{#snippet projectRow(project: Project, scope: string = ROOT_SCOPE)}
  <Collapsible
    class="min-w-0 items-stretch"
    open={project.expanded}
    onOpenChange={({ open }) => persistProjectExpansion(project, open)}
  >
    <div
      class="group/project relative flex items-center rounded-md {dropClass({ kind: "item", id: project.path })}"
      role="presentation"
      {...contextMenuTrigger(projectTarget(project))}
      use:draggable={dragSource(scope, { kind: "item", id: project.path }, project.name)}
    >
      <Collapsible.Trigger
        class="group flex min-w-0 flex-1 items-center gap-2 rounded-md py-1.5 pl-2 pr-14 text-left text-[13px] font-medium group-hover/project:preset-tonal"
      >
        <ChevronDown class="text-surface-500 transition group-data-[state=closed]:-rotate-90" size={14} />
        {#if project.kind === "multiProject"}
          <Layers3 class="text-primary-500" size={15} strokeWidth={1.7} />
        {:else if project.kind === "worktree"}
          <TooltipAnchor label="Codex worktree" class="grid place-items-center">
            <FolderGit2 class="text-primary-500" size={15} strokeWidth={1.7} />
          </TooltipAnchor>
        {:else}
          <Folder class="text-surface-500" size={15} strokeWidth={1.7} />
        {/if}
        <TooltipAnchor
          label={projectTooltip(project)}
          multiline={true}
          role="presentation"
          aria-label={projectTooltip(project)}
          class="min-w-0 flex-1 truncate"
          onmouseenter={() => ensureGitStatus(project.path)}
          >{project.name}</TooltipAnchor
        >
        <span class="shrink-0 text-[10px] text-surface-500">{projectHostLabel(project.path)}</span>
        {#if onAddRepository}
          <RepositoryMark path={project.path} />
        {/if}
        {#if project.pinned}
          <Pin class="shrink-0 text-surface-500" size={11} />
        {/if}
        <span class="text-[10px] font-normal text-surface-500 group-hover/project:opacity-0"
          >{project.threads.length}</span
        >
      </Collapsible.Trigger>
      <TooltipButton
        label={`New thread in ${project.name}`}
        onclick={(event) => {
          event.stopPropagation();
          onNewThread(project);
        }}
        class="absolute right-7 top-1/2 size-6 -translate-y-1/2 row-action group-hover/project:opacity-100"
      >
        <SquarePen size={13} />
      </TooltipButton>
      <button
        type="button"
        aria-label="Project menu"
        {...menuButton(projectTarget(project))}
        class="absolute right-1 top-1/2 size-6 -translate-y-1/2 row-action group-hover/project:opacity-100"
      >
        <Ellipsis size={14} />
      </button>
      {#if onOpenPullRequests && (needsYou[project.path] ?? 0) > 0}
        <button
          type="button"
          class="status-badge preset-tonal-warning ml-1 shrink-0 group-hover/project:hidden"
          aria-label="{needsYou[project.path]} need you"
          title="{needsYou[project.path]} pull requests need you in {project.name}"
          onclick={() => onOpenPullRequests?.(project.path)}
        >
          {needsYou[project.path]}
        </button>
      {/if}
      {#if onOpenWorktrees && project.kind !== "multiProject"}
        <div class="ml-1 shrink-0 group-hover/project:hidden">
          <BranchChip path={project.path} onOpen={() => onOpenWorktrees?.(project)} maxWidthClass="max-w-[6rem]" />
        </div>
      {/if}
    </div>
    <Collapsible.Content class="ml-[27px] border-l border-surface-200-800 pl-1.5">
      {#if reviewThreads(project).length > 0}
        <div data-testid="reviews-group" class="mb-0.5">
          <div class="eyebrow px-2 pb-0.5 pt-1.5 text-[10px]">Reviews</div>
          {#each reviewThreads(project) as row (row.thread.id)}
            {@render threadRow(project, row.thread, reviewsScope(project.path))}
          {/each}
        </div>
      {/if}
      {#each worktreeChildren(project) as child (child.path)}
        {#if child.adopted}
          {@render projectRow(child.adopted, nestedScope(project.path))}
        {:else}
          {@render unaddedRow(project, child)}
        {/if}
      {/each}
      {#if project.threads.length === 0}
        {#if worktreeChildren(project).length === 0 && reviewThreads(project).length === 0}
          <p class="px-2 py-2 text-xs text-surface-500">No Codex threads yet</p>
        {/if}
      {:else}
        {@const hidden = splitThreads(project).hidden}
        {@const shownExtra = Math.min(hidden, revealed[project.path] ?? 0)}
        {@const remaining = hidden - shownExtra}
        {@render threadNodes(project, projectTree(project))}
        {#if remaining > 0}
          <button
            type="button"
            onclick={(event) => revealMore(project, event.metaKey || event.ctrlKey)}
            class="mt-0.5 flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-[11px] font-medium text-primary-500 hover:preset-tonal"
          >
            <ChevronDown class="-rotate-90 transition" size={12} />
            Show {modifierHeld ? remaining : Math.min(REVEAL_STEP, remaining)} more
          </button>
        {:else if shownExtra > 0}
          <button
            type="button"
            onclick={() => (revealed[project.path] = 0)}
            class="mt-0.5 flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-[11px] font-medium text-primary-500 hover:preset-tonal"
          >
            <ChevronDown class="transition" size={12} />
            Show less
          </button>
        {/if}
      {/if}
    </Collapsible.Content>
  </Collapsible>
{/snippet}

{#snippet unaddedRow(repo: Project, child: WorktreeChild)}
  <Collapsible
    class="min-w-0 items-stretch"
    open={unaddedOpen[child.path] ?? false}
    onOpenChange={({ open }) => (unaddedOpen[child.path] = open)}
  >
    <div class="group/unadded relative flex items-center rounded-md" data-testid="unadded-worktree">
      <Collapsible.Trigger
        class="group flex min-w-0 flex-1 items-center gap-2 rounded-md py-1.5 pl-2 pr-14 text-left text-[13px] text-surface-600-400 group-hover/unadded:preset-tonal"
      >
        <ChevronDown class="text-surface-500 transition group-data-[state=closed]:-rotate-90" size={14} />
        <TooltipAnchor label="Worktree not added to the sidebar" class="grid place-items-center">
          <FolderGit2 class="text-surface-500" size={15} strokeWidth={1.7} />
        </TooltipAnchor>
        <TooltipAnchor
          label={`${child.path}\n⎇ ${child.branch}`}
          multiline={true}
          role="presentation"
          aria-label={child.path}
          class="min-w-0 flex-1 truncate italic"
          >{child.name}</TooltipAnchor
        >
        <span class="max-w-[6rem] shrink-0 truncate text-[10px] text-surface-500 group-hover/unadded:opacity-0"
          >{child.branch}</span
        >
        <span class="text-[10px] font-normal text-surface-500 group-hover/unadded:opacity-0"
          >{child.threads.length}</span
        >
      </Collapsible.Trigger>
      {#if onNewThreadInDir}
        <TooltipButton
          label={`New thread in ${child.name}`}
          onclick={(event) => {
            event.stopPropagation();
            onNewThreadInDir(child.path);
          }}
          class="absolute right-7 top-1/2 size-6 -translate-y-1/2 row-action group-hover/unadded:opacity-100"
        >
          <SquarePen size={13} />
        </TooltipButton>
      {/if}
      {#if onAddWorktreeProject}
        <TooltipButton
          label="Add to sidebar"
          onclick={(event) => {
            event.stopPropagation();
            onAddWorktreeProject(child.path);
          }}
          class="absolute right-1 top-1/2 size-6 -translate-y-1/2 row-action group-hover/unadded:opacity-100"
        >
          <Plus size={14} />
        </TooltipButton>
      {/if}
    </div>
    <Collapsible.Content class="ml-[27px] border-l border-surface-200-800 pl-1.5">
      {#if child.threads.length === 0}
        <p class="px-2 py-2 text-xs text-surface-500">No threads yet</p>
      {:else}
        {#each child.threads as thread (thread.id)}
          {@render threadRow(repo, thread, nestedScope(child.path))}
        {/each}
      {/if}
    </Collapsible.Content>
  </Collapsible>
{/snippet}

{#snippet rootNode(node: TreeNode<Project>)}
  {#if node.kind === "folder"}
    {@render folderNode(node.folder, null, node.children)}
  {:else}
    {@render projectRow(node.item)}
  {/if}
{/snippet}

<aside class="flex h-full w-full flex-col border-r border-surface-200-800 bg-surface-100-900 text-surface-900-100">
  <div class="h-7 shrink-0 select-none" data-tauri-drag-region use:dragRegion></div>
  <div class="flex h-12 items-center justify-between px-3 select-none" data-tauri-drag-region use:dragRegion>
    <button
      onclick={() => onGoHome?.()}
      class="flex items-center gap-2.5 rounded-lg font-semibold tracking-[-0.02em] hover:opacity-80"
      title="Home"
    >
      <div class="grid size-7 place-items-center rounded-lg preset-filled-primary-500 text-[11px] font-bold">P</div>
      Pingex
    </button>
    <div class="flex items-center gap-0.5">
      <TooltipButton
        label="Search threads (⌘⇧F)"
        aria-label="Search threads"
        onclick={openSearch}
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-600-400"
      >
        <Search size={16} strokeWidth={1.8} />
      </TooltipButton>
      <TooltipButton
        label="New thread"
        onclick={() => onNewThread()}
        disabled={visibleProjects.length === 0}
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-600-400 disabled:opacity-40"
      >
        <SquarePen size={17} strokeWidth={1.8} />
      </TooltipButton>
    </div>
  </div>

  {#if onOpenPullRequests}
    {@const needsYouTotal = Object.values(needsYou).reduce((sum, count) => sum + count, 0)}
    <div class="px-2 pt-1">
      <button
        type="button"
        onclick={() => onOpenPullRequests()}
        data-testid="sidebar-pull-requests"
        aria-current={pullRequestsOpen ? "page" : undefined}
        class="flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs transition {pullRequestsOpen
          ? "preset-tonal-primary"
          : "text-surface-600-400 hover:preset-tonal"}"
      >
        <GitPullRequest size={14} />
        <span class="grow">Pull requests</span>
        {#if needsYouTotal > 0}
          <span class="status-badge preset-tonal-warning">{needsYouTotal}</span>
        {/if}
      </button>
    </div>
  {/if}

  {#if searching}
    <SidebarSearch
      {projects}
      currentProjectPath={currentProject?.path ?? null}
      currentProjectName={currentProject?.name ?? null}
      onOpenResult={(item) => onOpenSearchResult?.(item)}
      onClose={() => (searching = false)}
    />
  {:else}
    <div class="group/projects min-h-0 flex-1 select-none overflow-y-auto px-2 pb-3 pt-3">
      <div class="projects-heading mb-1 flex h-8 items-center justify-between px-2">
        <span class="eyebrow">Projects</span>
        <div class="flex items-center gap-0.5">
          {#each headerActions as action (action.label)}
            <TooltipButton
              label={action.label}
              onclick={action.onclick}
              data-testid={action.testId}
              aria-pressed={action.active}
              class="btn-icon btn-icon-sm transition focus:opacity-100 group-hover/projects:opacity-100 {action.active
                ? "preset-tonal-primary opacity-100"
                : "hover:preset-tonal text-surface-600-400 opacity-0"}"
            >
              <action.icon size={15} />
            </TooltipButton>
          {/each}
        </div>
      </div>

      {#if loading}
        <div class="space-y-2 px-2 py-2" aria-label="Loading projects">
          <div class="placeholder h-8 animate-pulse rounded-md"></div>
          <div class="placeholder h-8 animate-pulse rounded-md opacity-70"></div>
        </div>
      {:else if visibleProjects.length === 0}
        <button onclick={onAddProject} class="mx-2 mt-2 empty-cta rounded-lg px-3 py-4">
          Add a folder to see its Codex threads here.
        </button>
      {:else}
        <div class="space-y-0.5">
          {#each rootTree as node (rowId(node))}
            {@render rootNode(node)}
          {/each}
        </div>
      {/if}
      {#if onAddRepository && !loading}
        <SidebarRepositories {onAddRepository} />
      {/if}
    </div>

    <ArchivedThreadsSection {onSelectArchived} {onUnarchived} />
  {/if}

  <div class="border-t border-surface-200-800 p-2">
    {#if connectedDevices}
      <button
        onclick={onOpenSettings}
        title="Manage remote connections"
        class="mb-1 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-[11px] text-surface-600-400 transition hover:preset-tonal"
      >
        <span class="relative grid size-4 place-items-center">
          <Smartphone size={13} class="text-success-600 dark:text-success-400" />
          <span class="absolute -right-0.5 -top-0.5 size-1.5 rounded-full bg-success-500"></span>
        </span>
        <span class="min-w-0 flex-1 truncate">
          {#if onlineCount > 0}
            {onlineCount}
            device{onlineCount === 1 ? "" : "s"}
            online
          {:else}
            Device recently active
          {/if}
        </span>
      </button>
    {/if}
    {#if onOpenMemories}
      <button
        onclick={onOpenMemories}
        data-testid="sidebar-memories"
        aria-current={memoriesOpen ? "page" : undefined}
        class="mb-1 flex w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-xs transition {memoriesOpen
          ? "preset-tonal-primary"
          : "text-surface-600-400 hover:preset-tonal"}"
      >
        <Brain size={14} />
        Memories
      </button>
    {/if}
    <UsageMeter snapshot={accountUsage.snapshot} compact />
    <button
      onclick={onOpenSettings}
      class="group flex w-full items-center gap-2.5 rounded-lg p-2 text-left transition hover:preset-tonal"
    >
      <div class="grid size-8 shrink-0 place-items-center rounded-full preset-filled-primary-500 text-xs font-semibold">
        {(account?.label ?? "?").slice(0, 1).toUpperCase()}
      </div>
      <div class="min-w-0 flex-1">
        <div class="truncate text-xs font-medium">{account?.label ?? "Not signed in"}</div>
        <div class="mt-0.5 text-[10px] capitalize text-surface-500">
          {account?.plan ?? account?.kind ?? "Codex account"}
        </div>
      </div>
      <Settings size={15} class="text-surface-500 transition group-hover:text-surface-800-200" />
    </button>
  </div>
</aside>

{#if dnd.dragging}
  <div
    class="pointer-events-none fixed z-50 max-w-[220px] truncate rounded-md border border-surface-200-800 bg-surface-50-950 px-2 py-1 text-[12px] shadow-lg"
    style="left: {dnd.x + 12}px; top: {dnd.y + 8}px"
    aria-hidden="true"
  >
    {dnd.dragging.label}
  </div>
{/if}

<SidebarContextMenu menu={contextMenu} target={menuTarget} {sectionsSupported} />
<SidebarContextMenu menu={buttonMenu} target={menuTarget} {sectionsSupported} />

<style>
  :global(.drop-before) {
    box-shadow: inset 0 2px 0 0 var(--color-primary-500);
  }
  :global(.drop-after) {
    box-shadow: inset 0 -2px 0 0 var(--color-primary-500);
  }
  :global(.drop-inside) {
    box-shadow: inset 0 0 0 2px var(--color-primary-500);
  }
</style>
