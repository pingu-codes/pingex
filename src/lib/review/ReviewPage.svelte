<!--
  The review page for one PR: every changed file on one scrolling page, each
  collapsible and marked viewed (per profile, by blob id); a file tree with
  comment counts; inline review threads; line and range comments and
  suggestions; and the pending review, submitted once as one GitHub review.

  Slots for agent reviews: `findingCounts` (per path) shows the Agent
  findings filter and the finding counts, `lineAnnotations` renders under
  each diff line (finding cards) and `annotatedLines` says which lines it
  fills, `agentReview` goes in the Your review
  panel, and Findings join the review through `PendingReviewStore.add`.
-->
<script lang="ts">
  import { ArrowUp, ChevronLeft, PanelRightOpen } from "@lucide/svelte";
  import { type Snippet, tick, untrack } from "svelte";
  import { dialogStack, openDialog } from "$lib/app/dialogs.svelte";
  import type { PrMove } from "$lib/app/navigation.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { peopleByLogin } from "$lib/inbox/inbox";
  import { stepHint } from "$lib/review/actions";
  import ChecksTab from "$lib/review/ChecksTab.svelte";
  import ConversationTab from "$lib/review/ConversationTab.svelte";
  import FileDiff, { type Composer, type LineSlot } from "$lib/review/FileDiff.svelte";
  import FileRail from "$lib/review/FileRail.svelte";
  import FindBar from "$lib/review/FindBar.svelte";
  import type { PendingReviewStore } from "$lib/review/pendingReview.svelte";
  import { mergeMethod } from "$lib/review/prActions";
  import ReviewHeader from "$lib/review/ReviewHeader.svelte";
  import ReviewThread, { type HandTarget } from "$lib/review/ReviewThread.svelte";
  import { type CommentThread, commentThreads, conversationTimeline } from "$lib/review/review";
  import { ReviewFiles } from "$lib/review/reviewFiles.svelte";
  import { type FindMatch, type FindText, type FindUnit, fileFindUnits, PATH_UNIT } from "$lib/review/reviewFind";
  import {
    type AskAgentRequest,
    type DiffSide,
    entryLocation,
    type FileCounts,
    fileBlob,
    fileCounts,
    fileInView,
    groupBy,
    type LineRange,
    prPlace,
    queueNeighbour,
    type ReviewFilter,
    rangeAnchor,
    rangeText,
    reuseUnchanged,
    reviewKey,
    sameFields,
    sameItems,
    selectLine,
    threadShown,
    upNext,
  } from "$lib/review/reviewPage";
  import { loadReviewCollapsed, saveReviewCollapsed } from "$lib/review/reviewPrefs";
  import WholeFileDialog from "$lib/review/WholeFileDialog.svelte";
  import WorktreeBadge from "$lib/review/WorktreeBadge.svelte";
  import YourReview from "$lib/review/YourReview.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { prefetch, query } from "$lib/services/queryCache.svelte";
  import BranchNameDialog from "$lib/stack/BranchNameDialog.svelte";
  import { baseInput, driftLabel } from "$lib/stack/stack";
  import type {
    ChangesSince,
    CheckLog,
    CheckoutStep,
    GitHubPerson,
    Inbox,
    PendingEntry,
    PrCheckout,
    PrComment,
    PrDetail,
    PrFile,
  } from "$lib/types";
  import { submitState } from "$lib/utils/actionState.svelte";
  import { errorMessage } from "$lib/utils/errors";
  import { shortSha } from "$lib/utils/format";
  import { basename } from "$lib/utils/path";

  let {
    repoDir,
    number,
    onBack,
    backLabel = "Pull requests",
    inbox = null,
    onOpenPr,
    onAskAgent,
    onRunLocally,
    headerActions,
    step = "noCheckout",
    checkout = null,
    checkoutKnown = false,
    findingCounts,
    lineAnnotations,
    annotatedLines,
    annotationText,
    agentReview,
    threadsTab,
    pending,
    prThreads = [],
  }: {
    repoDir: string;
    number: number;
    /** The breadcrumb's way back; without it (inside the Stack page, which
     *  has its own) there is no breadcrumb. */
    onBack?: () => void;
    /** Names where `onBack` leads. */
    backLabel?: string;
    /** The repository's last inbox sync: the viewer, Up next and j/k. */
    inbox?: Inbox | null;
    onOpenPr?: (number: number, move?: PrMove) => void;
    onAskAgent?: (request: AskAgentRequest) => void;
    /** Run locally or Open in editor: creates a worktree when the PR has none.
     *  Without it both are unavailable. */
    onRunLocally?: (detail: PrDetail, action: "runLocally" | "openInEditor") => void;
    /** More of the header's actions, beside Run locally. */
    headerActions?: Snippet;
    /** The checkout step the PR is at. */
    step?: CheckoutStep;
    /** Where the PR is on disk, for the header's badge. */
    checkout?: PrCheckout | null;
    /** `checkout` has been read: `null` then means no worktree. */
    checkoutKnown?: boolean;
    /** Agent Findings per path; shows the Agent findings filter. */
    findingCounts?: Record<string, number>;
    lineAnnotations?: Snippet<[LineSlot]>;
    /** Per path, the lines (`side:line`) `lineAnnotations` fills. */
    annotatedLines?: ReadonlyMap<string, ReadonlySet<string>>;
    /** The text `lineAnnotations` shows at a line, for Cmd+F. */
    annotationText?: (slot: LineSlot) => FindText[];
    agentReview?: Snippet;
    /** The Threads tab: the threads started on this PR. */
    threadsTab?: Snippet;
    /** The pending review, which the caller adds to as well (agent Findings). */
    pending: PendingReviewStore;
    /** Threads already on this PR: "Hand to agent" offers them. */
    prThreads?: HandTarget[];
  } = $props();

  const NO_THREADS: CommentThread[] = [];
  const NO_PENDING: PendingEntry[] = [];
  const NO_COUNTS: FileCounts = { comments: 0, unresolved: 0, findings: 0 };
  /** A chosen current file survives scrolling by less than a diff row. */
  const HOLD_SLOP = 20;

  // Cached: a remount (j/k, back from a thread, a branch click on the Stack
  // page) shows the PR at once and refreshes it behind. A write through
  // `commands` marks it stale, which re-reads it.
  const detailQuery = query("reviewPrDetail", () => [repoDir, number]);
  const detail = $derived<PrDetail | null>(detailQuery.value ?? null);
  // How far GitHub's head is behind GitHub's base tip, for the header badge.
  const behindQuery = query("reviewPrsBehind", () =>
    detail
      ? [
          repoDir,
          [baseInput({ number, headSha: detail.headSha, baseRef: detail.summary.baseRef, baseSha: detail.baseSha })],
        ]
      : null,
  );
  const drift = $derived(
    detail ? driftLabel({ number, baseRef: detail.summary.baseRef }, behindQuery.value?.[0]?.behind, inbox) : null,
  );
  const error = $derived(detail ? null : detailQuery.error);
  // svelte-ignore state_referenced_locally -- the page is keyed by PR.
  const left = prPlace(repoDir, number);
  /** The page opens on "since", as it was left or because the inbox asked. */
  const opensOnSince = left.changes === "since";
  /** Where to land: the scroll position recorded as the page loads may be clamped. */
  const landAt = left.scrollTop;
  let tab = $state(left.tab);
  let filter = $state(left.filter);
  let collapsedDirs = $state(left.collapsedDirs);
  const reviewFiles = new ReviewFiles(left.collapsed);
  const { viewed, collapsed } = reviewFiles;
  $effect(() => {
    left.tab = tab;
    left.filter = filter;
    left.collapsedDirs = collapsedDirs;
    left.changes = changes;
    if (reviewFiles.collapsedKnown) left.collapsed = [...collapsed];
  });
  let selection = $state<LineRange | null>(null);
  let composer = $state<Composer | null>(null);
  let currentPath = $state<string | null>(null);
  let helpOpen = $state(false);
  /** Below the rail's width the Your review panel is an overlay, shown on demand. */
  let reviewOpen = $state(false);
  /** At the rail's width, the Your review panel folded to a thin strip. The
   *  page remounts per PR, so the choice is stored to carry between them. */
  let reviewCollapsed = $state(loadReviewCollapsed());
  let scroller = $state<HTMLElement>();
  /** The topbar has scrolled fully out of the page. */
  let topbarHidden = $state(false);
  let topbarHeight = $state(0);
  let changes = $state<"all" | "since">("all");
  let since = $state<ChangesSince | null>(null);
  let checkLogs = $state<CheckLog[] | null>(null);
  let lastUnresolved = -1;

  // svelte-ignore state_referenced_locally -- the page is keyed by PR.
  const action = submitState();

  const viewer = $derived(inbox?.viewer ?? "");
  /** GitHub refuses Approve and Request changes from a PR's author. */
  const ownPr = $derived(!!detail && !!viewer && detail.summary.author === viewer);
  const queue = $derived(upNext(inbox, number));
  const people = $derived(inbox ? peopleByLogin(inbox) : new Map<string, GitHubPerson>());
  const files = $derived<PrFile[]>(changes === "since" && since ? since.files : (detail?.files ?? []));
  const threads = $derived(commentThreads(detail?.comments ?? []));
  const shownThreads = $derived(threads.filter((thread) => threadShown(thread, filter)));
  // Shares `threads` with the Files tab, so a reply or resolve shows in both.
  const conversation = $derived(conversationTimeline(detail?.comments ?? [], detail?.reviews ?? [], threads));
  // Per path, keeping a path's array or counts when they did not change, so a
  // comment or pending entry re-renders only its own file's rows.
  let lastThreads = new Map<string, CommentThread[]>();
  const threadsByPath = $derived.by(
    () =>
      (lastThreads = reuseUnchanged(
        lastThreads,
        groupBy(shownThreads, (thread) => thread.path),
        sameItems,
      )),
  );
  let lastPending = new Map<string, PendingEntry[]>();
  const pendingByPath = $derived.by(() => {
    // With finding cards in the diff, an added Finding shows on its card.
    const inline = pending.entries.filter((entry) => !(lineAnnotations && entry.kind === "acceptedFinding"));
    lastPending = reuseUnchanged(
      lastPending,
      groupBy(inline, (entry) => entryLocation(entry).path),
      sameItems,
    );
    return lastPending;
  });
  let lastCounts = new Map<string, FileCounts>();
  const countsByPath = $derived.by(() => {
    const own = groupBy(threads, (thread) => thread.path);
    const next = new Map(
      files.map((file) => [
        file.path,
        fileCounts(own.get(file.path) ?? [], file.path, findingCounts?.[file.path] ?? 0),
      ]),
    );
    lastCounts = reuseUnchanged(lastCounts, next, sameFields);
    return lastCounts;
  });
  const viewedCount = $derived(files.filter((file) => viewed.has(file.path)).length);
  const unresolvedCount = $derived(threads.filter((thread) => !thread.resolved).length);
  const stale = $derived(!!detail && !!pending.headSha && pending.headSha !== detail.headSha);

  /** The head the viewed marks were last read at. */
  let readHead: string | null = null;

  // Each time the detail lands (first read, a refresh, after a write).
  $effect(() => {
    const next = detail;
    if (!next) return;
    untrack(() => {
      currentPath ??= next.files[0]?.path ?? null;
      pending.syncHead(next.headSha);
      if (readHead === next.headSha) return;
      const first = readHead === null;
      readHead = next.headSha;
      if (first && !opensOnSince) land();
      void loadReviewState(next, first);
      if (first) prefetchNext();
      if (first ? opensOnSince : changes === "since") void showChanges("since");
    });
  });

  /** The ? key opens the shortcuts wherever the page is scrolled: under their
   *  button, or at the page's corner once the topbar has gone. */
  function shortcutsAnchor(
    button: { getBoundingClientRect(): { x: number; y: number; width: number; height: number } } | null,
  ) {
    if (button && !topbarHidden) return button.getBoundingClientRect();
    const page = scroller?.getBoundingClientRect();
    return page ? { x: page.right - 20, y: page.top + 2, width: 0, height: 0 } : null;
  }

  /** Back to where the page was left. */
  function land() {
    if (scroller && landAt > 0) scroller.scrollTop = landAt;
  }

  /** The stored pending review and the viewed marks, read side by side. */
  async function loadReviewState(next: PrDetail, first: boolean) {
    const [, marked] = await Promise.all([
      pending.loaded ? Promise.resolve() : pending.load(next.headSha),
      commands.reviewViewedFiles(repoDir, number, next.files.map(fileBlob)).catch(() => [] as string[]),
    ]);
    if (detail) pending.syncHead(detail.headSha);
    if (readHead !== next.headSha) return;
    reviewFiles.applyMarks(marked);
    // The pending review's cards and the folds above change the page's height.
    if (first && !opensOnSince) land();
  }

  /** Warm the next PR in the queue, so j/k opens it at once. Only one: each
   *  read is several GitHub calls. */
  function prefetchNext() {
    const next = queueNeighbour(queue, number, 1);
    if (next) prefetch("reviewPrDetail", repoDir, next.number);
  }

  /** Re-read the PR after a write, keeping what the reviewer had open. The
   *  write already marked it stale, so this joins that read. */
  async function reload() {
    await detailQuery.reload();
    if (detailQuery.error) action.error = detailQuery.error;
  }

  // --- scrolling and the Your review panel ---------------------------------

  /** From the scroll event and a height kept by a ResizeObserver, so nothing
   *  forces layout. Not a second IntersectionObserver on this scroller: in
   *  WebKit that splits `nearViewport`'s row swap across frames, a layout lands
   *  on the half-swapped page, and the scroll position clamps upward. */
  function onPageScroll() {
    if (!scroller) return;
    topbarHidden = topbarHeight > 0 && scroller.scrollTop >= topbarHeight;
    // "Changes since" is a different, shorter page and doesn't come back.
    if (changes === "all") left.scrollTop = scroller.scrollTop;
    scheduleCurrentFile();
  }

  // The current file follows the page. Measured in a frame, where a read costs
  // no layout the frame would not do anyway and no Svelte flush is half done.
  let currentFileFrame = 0;
  /** Where the page was when the reader chose the current file, or null. */
  let heldAt: number | null = null;
  let lastScrollTop = 0;

  function scheduleCurrentFile() {
    currentFileFrame ||= requestAnimationFrame(updateCurrentFile);
  }

  function updateCurrentFile() {
    currentFileFrame = 0;
    if (!scroller) return;
    const top = scroller.scrollTop;
    const movedDown = top > lastScrollTop;
    lastScrollTop = top;
    if (heldAt !== null && Math.abs(top - heldAt) <= HOLD_SLOP && !(currentPath && collapsed.has(currentPath))) return;
    heldAt = null;
    const elements = scroller.querySelectorAll<HTMLElement>("[data-file]");
    const view = scroller.getBoundingClientRect();
    // Short files at the end never reach the top: the end of the page is theirs.
    const atEnd = movedDown && top + scroller.clientHeight >= scroller.scrollHeight - 1;
    const index = atEnd
      ? elements.length - 1
      : fileInView(
          elements.length,
          (at) => {
            const box = elements[at].getBoundingClientRect();
            return { top: box.top, bottom: box.bottom, open: !collapsed.has(elements[at].dataset.file ?? "") };
          },
          view.top,
          view.bottom,
        );
    const path = elements[index]?.dataset.file;
    if (path) currentPath = path;
  }

  // What is in view also changes when a file folds or the list changes.
  $effect(() => {
    void files;
    void tab;
    void collapsed.size;
    untrack(scheduleCurrentFile);
    return () => {
      cancelAnimationFrame(currentFileFrame);
      currentFileFrame = 0;
    };
  });

  /** The reader chose this file: it stays current until the page moves on. */
  function holdCurrent(path: string) {
    currentPath = path;
    heldAt = scroller?.scrollTop ?? 0;
  }

  /** Where a file's top sits from the top of the view. Forces layout: call it
   *  from an event or after `tick`, not on scroll. */
  function fileOffset(path: string): number | null {
    const element = scroller?.querySelector(`[data-file="${CSS.escape(path)}"]`);
    if (!scroller || !element) return null;
    return element.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
  }

  /** Fold or unfold files and keep the reader's place. WebKit has no scroll
   *  anchoring, so otherwise folding a file whose header is above the view
   *  drops the reader files further down. That file's header comes back to the
   *  top of the view; any other file stays where it is on screen. */
  async function keepingPlace(change: () => void) {
    const path = currentPath;
    const before = path ? fileOffset(path) : null;
    change();
    if (path === null || before === null) return;
    await tick();
    const after = fileOffset(path);
    if (scroller && after !== null) {
      scroller.scrollTop += after - (before < 0 && collapsed.has(path) ? 0 : before);
    }
  }

  /** At once, not smoothly: a smooth scroll would mount every file it passes. */
  function scrollToTop() {
    if (scroller) scroller.scrollTop = 0;
  }

  function setReviewCollapsed(value: boolean) {
    reviewCollapsed = value;
    reviewOpen = false;
    saveReviewCollapsed(value);
  }

  // --- files ---------------------------------------------------------------

  function setViewed(targets: PrFile[], value: boolean) {
    void keepingPlace(() => reviewFiles.setViewed(targets, value));
    if (targets.length > 0) {
      commands.reviewSetViewed(repoDir, number, targets.map(fileBlob), value).catch((cause) => {
        action.error = errorMessage(cause);
      });
    }
  }

  function toggle(file: PrFile, event: MouseEvent) {
    holdCurrent(file.path);
    void keepingPlace(() => {
      if (event.altKey) reviewFiles.openOnly(files, file.path);
      else reviewFiles.toggle(file.path);
    });
  }

  function jumpTo(path: string) {
    currentPath = path;
    reviewFiles.setOpen(path, true);
    tab = "files";
    queueMicrotask(() => {
      document.querySelector(`[data-file="${CSS.escape(path)}"]`)?.scrollIntoView?.({ block: "start" });
      // After the scroll: a file near the end can't reach the top of the view.
      holdCurrent(path);
    });
  }

  // Every file's menu is one of two, so marking one file viewed rebuilds none.
  const fileMenus = $derived({ viewed: fileMenu(true), unviewed: fileMenu(false) });

  function fileMenu(isViewed: boolean): MenuEntry[] {
    return [
      { id: "ask", label: "Ask agent about this file", disabled: !onAskAgent },
      { kind: "separator" },
      { id: "viewed", label: isViewed ? "Mark not viewed" : "Mark viewed", shortcut: "V" },
      { id: "above", label: "Mark this and above viewed", shortcut: "Shift+V" },
      { id: "others", label: "Collapse all others", hint: "⌥click" },
      { kind: "separator" },
      { id: "whole", label: "View whole file" },
      {
        id: "editor",
        label: "Open in editor",
        shortcut: "CmdOrCtrl+E",
        disabled: !onRunLocally,
        title: stepHint(step, "openInEditor") ?? undefined,
      },
      { id: "copy", label: "Copy path" },
    ];
  }

  function onFileMenu(file: PrFile, id: string) {
    holdCurrent(file.path);
    const index = files.indexOf(file);
    switch (id) {
      case "ask":
        askAgent({ path: file.path, side: null, startLine: null, line: null, text: file.patch, askOnly: false });
        break;
      case "viewed":
        setViewed([file], !viewed.has(file.path));
        break;
      case "above":
        setViewed(files.slice(0, index + 1), true);
        break;
      case "others":
        void keepingPlace(() => reviewFiles.openOnly(files, file.path));
        break;
      case "whole":
        if (detail) {
          void openDialog(WholeFileDialog, {
            repoDir,
            number,
            path: file.path,
            headSha: detail.headSha,
            commits: detail.commits,
          });
        }
        break;
      case "editor":
        if (detail) onRunLocally?.(detail, "openInEditor");
        break;
      case "copy":
        void commands.copyText(file.path);
        break;
    }
  }

  async function showChanges(value: "all" | "since") {
    if (value === "all" || !detail) {
      changes = "all";
      return;
    }
    const at = detail;
    await action.run(async () => {
      await commands.reviewFetchPrs(repoDir, [number]);
      const read = await commands.reviewChangesSince(repoDir, number, at.baseSha, at.headSha);
      if (detail?.headSha !== at.headSha) return;
      since = read;
      changes = "since";
    });
  }

  // --- lines and the pending review ----------------------------------------

  function onSelectLine(file: PrFile, side: DiffSide, line: number, extend: boolean) {
    holdCurrent(file.path);
    composer = null;
    selection = selectLine(selection, file.path, side, line, extend);
  }

  function compose(mode: Composer["mode"]) {
    if (selection) composer = { range: selection, mode };
  }

  function addEntry(body: string, replacement: string | null) {
    if (!composer) return;
    const anchor = rangeAnchor(composer.range);
    if (replacement !== null) pending.add({ kind: "suggestion", anchor, body, replacement });
    else pending.add({ kind: "comment", anchor, body });
    composer = null;
    selection = null;
  }

  function askAgent(request: Omit<AskAgentRequest, "number" | "title">) {
    onAskAgent?.({ ...request, number, title: detail?.summary.title ?? "" });
  }

  function askAboutSelection(file: PrFile) {
    if (!selection) return;
    askAgent({
      path: selection.path,
      side: selection.side,
      startLine: selection.start,
      line: selection.end,
      text: rangeText(file, selection),
      askOnly: false,
    });
  }

  async function submit() {
    if (!detail || action.busy) return;
    const head = detail.headSha;
    if (ownPr && pending.verdict !== "comment") pending.setVerdict("comment");
    const ok = await action.run(() => pending.submit());
    if (!ok) return;
    // What "changes since your review" measures from. Best effort: a
    // PRs-only repository has no clone to pin it in.
    void commands
      .reviewFetchPrs(repoDir, [number])
      .then(() => commands.reviewMarkSeen(repoDir, number, head))
      .catch(() => {});
    await reload();
  }

  // --- threads -------------------------------------------------------------

  async function replyNow(commentId: number, body: string): Promise<boolean> {
    return action.run(async () => {
      await commands.reviewReply(repoDir, number, commentId, body);
      await reload();
    });
  }

  function holdReply(thread: (typeof threads)[number], body: string) {
    const root = thread.comments[0];
    if (!root) return;
    pending.add({
      kind: "reply",
      threadId: thread.key,
      commentId: root.id,
      path: thread.path,
      line: thread.line,
      body,
    });
  }

  async function resolve(threadId: string) {
    await action.run(async () => {
      await commands.reviewResolveThread(repoDir, threadId);
      await reload();
    });
  }

  async function editComment(comment: PrComment, body: string): Promise<boolean> {
    return action.run(async () => {
      await commands.reviewEditComment(
        repoDir,
        { kind: comment.path ? "review" : "conversation", id: comment.id },
        body,
      );
      await reload();
    });
  }

  async function deleteComment(comment: PrComment) {
    const confirmed = await openDialog(ConfirmDialog, {
      title: "Delete this comment?",
      body: "It is removed from the pull request on GitHub.",
      confirmLabel: "Delete",
    });
    if (!confirmed) return;
    await action.run(async () => {
      await commands.reviewDeleteComment(repoDir, { kind: comment.path ? "review" : "conversation", id: comment.id });
      await reload();
    });
  }

  // --- the PR itself -------------------------------------------------------

  async function onPrMenu(id: string) {
    if (!detail) return;
    const at = detail;
    const method = mergeMethod(id);
    if (method) {
      const confirmed = await openDialog(ConfirmDialog, {
        title: `Merge #${number}?`,
        body: `Merges ${at.summary.headRef} into ${at.summary.baseRef} on GitHub, at the head you are reading.`,
        confirmLabel: "Merge",
        tone: "warning" as const,
      });
      if (confirmed) {
        await action.run(() => commands.reviewMerge(repoDir, number, { method, headSha: at.headSha }).then(reload));
      }
    } else if (id === "ready") {
      await action.run(() => commands.reviewMarkReady(repoDir, number).then(reload));
    } else if (id === "retarget") {
      const base = await openDialog(BranchNameDialog, {
        title: `Retarget #${number}`,
        subtitle: `Now into ${at.summary.baseRef}`,
        initial: at.summary.baseRef,
        label: "New base git branch",
        confirmLabel: "Retarget",
      });
      if (base) await action.run(() => commands.reviewRetarget(repoDir, number, base).then(reload));
    } else if (id === "close") {
      const confirmed = await openDialog(ConfirmDialog, {
        title: `Close #${number}?`,
        body: "The pull request closes without merging. It can be reopened on GitHub.",
        confirmLabel: "Close pull request",
      });
      if (confirmed) await action.run(() => commands.reviewClose(repoDir, number).then(reload));
    }
  }

  async function loadCheckLogs() {
    await action.run(async () => {
      checkLogs = await commands.reviewCheckLogs(repoDir, number);
    });
  }

  // --- find on this tab -----------------------------------------------------

  let findOpen = $state(false);
  let findQuery = $state("");
  let findBar = $state<FindBar>();
  /** The row Cmd+F's current match is in, kept mounted. */
  let findRow = $state<{ path: string; row: string } | null>(null);
  /** Where focus goes back to when the bar closes. */
  let findReturnFocus: Element | null = null;

  /** What the bar searches. Files lists its text, including rows that aren't
   *  mounted; the other tabs are all in the DOM, so `null` has it read that. */
  const findUnits = $derived.by<FindUnit[] | null>(() => {
    if (tab === "threads") return [];
    if (tab !== "files") return null;
    return files.flatMap((file) => {
      const lines = annotatedLines?.get(file.path);
      return fileFindUnits(
        file,
        threadsByPath.get(file.path) ?? NO_THREADS,
        pendingByPath.get(file.path) ?? NO_PENDING,
        annotationText &&
          ((side, line) => (lines?.has(`${side}:${line}`) ? annotationText({ path: file.path, side, line }) : [])),
      );
    });
  });

  function openFind() {
    const selected = window.getSelection()?.toString() ?? "";
    if (selected && selected.length <= 200 && !selected.includes("\n")) findQuery = selected;
    helpOpen = false;
    if (findOpen) {
      findBar?.focus();
      return;
    }
    findReturnFocus = document.activeElement;
    findOpen = true;
  }

  function closeFind() {
    findOpen = false;
    findRow = null;
    if (findReturnFocus instanceof HTMLElement && findReturnFocus.isConnected) findReturnFocus.focus();
    findReturnFocus = null;
  }

  /** Reaching a match opens its file, without touching its viewed mark. A match
   *  on the path shows in the header, so that one needn't. */
  function revealFindMatch(match: FindMatch) {
    const { path, id, row } = match.unit;
    if (!path) return;
    holdCurrent(path);
    if (id !== PATH_UNIT) reviewFiles.setOpen(path, true);
    findRow = row ? { path, row } : null;
  }

  // --- the keyboard --------------------------------------------------------

  function nextUnresolved() {
    const unresolved = files.flatMap((file) =>
      threads.filter((thread) => thread.path === file.path && !thread.resolved),
    );
    if (unresolved.length === 0) return;
    lastUnresolved = (lastUnresolved + 1) % unresolved.length;
    const thread = unresolved[lastUnresolved];
    jumpTo(thread.path);
    queueMicrotask(() => {
      document.getElementById(`thread-${thread.key}`)?.scrollIntoView?.({ block: "center" });
      holdCurrent(thread.path);
    });
  }

  function onKeydown(event: KeyboardEvent) {
    if (dialogStack.length > 0 || event.defaultPrevented) return;
    const command = reviewKey(event);
    if (!command) return;
    const index = files.findIndex((file) => file.path === currentPath);
    const current = files[index];
    event.preventDefault();
    switch (command) {
      case "nextPr":
      case "previousPr": {
        const next = queueNeighbour(queue, number, command === "nextPr" ? 1 : -1);
        if (next) onOpenPr?.(next.number, "walk");
        break;
      }
      case "nextFile":
      case "previousFile": {
        const next = files[Math.min(files.length - 1, Math.max(0, index + (command === "nextFile" ? 1 : -1)))];
        if (next) jumpTo(next.path);
        break;
      }
      case "nextUnresolved":
        nextUnresolved();
        break;
      case "toggleViewed":
        if (current) setViewed([current], !viewed.has(current.path));
        break;
      case "comment":
        compose("comment");
        break;
      case "submit":
        void submit();
        break;
      case "find":
        openFind();
        break;
      case "findNext":
        findBar?.next();
        break;
      case "findPrevious":
        findBar?.previous();
        break;
      case "help":
        helpOpen = !helpOpen;
        break;
    }
  }

  const TABS = $derived([
    { value: "conversation" as const, label: `Conversation ${conversation.length}` },
    { value: "files" as const, label: `Files ${detail?.files.length ?? 0}` },
    { value: "checks" as const, label: "Checks" },
    { value: "threads" as const, label: "Threads" },
  ]);
  const FILTERS = $derived<{ value: ReviewFilter; label: string }[]>([
    { value: "all", label: "All comments" },
    { value: "unresolved", label: `Unresolved ${unresolvedCount}` },
    ...(findingCounts ? [{ value: "findings" as const, label: "Agent findings" }] : []),
  ]);
</script>

<svelte:window onkeydown={onKeydown} />

<!-- A size container: the page lays out by its own width, so it also fits beside the Stack page's
     tree, and the sticky rails measure the page's height (100cqh). -->
<div class="relative h-full min-h-0 text-xs [container:review/size]">
  <!-- One scroller for the topbar and the panes, so the topbar scrolls away with the code. -->
  <div
    bind:this={scroller}
    data-diff-pane
    onscroll={onPageScroll}
    class="flex h-full flex-col overflow-x-hidden overflow-y-auto"
  >
    <ReviewHeader
      bind:height={topbarHeight}
      bind:helpOpen
      {onBack}
      {backLabel}
      repoName={basename(repoDir)}
      {detail}
      {number}
      {people}
      {drift}
      {onPrMenu}
      {tab}
      tabs={TABS}
      onTabChange={(value) => {
        tab = value;
        scrollToTop();
      }}
      {filter}
      filters={FILTERS}
      onFilterChange={(value) => (filter = value)}
      {changes}
      onShowChanges={showChanges}
      {viewedCount}
      fileCount={files.length}
      onExpandAll={() => keepingPlace(() => reviewFiles.expandAll())}
      onCollapseAll={() => keepingPlace(() => reviewFiles.collapseAll(files))}
      {reviewOpen}
      onToggleReview={() => (reviewOpen = !reviewOpen)}
      pendingCount={pending.entries.length}
      {shortcutsAnchor}
    >
      {#snippet actions()}
        <WorktreeBadge variant="chip" {checkout} known={checkoutKnown} />
        {@render headerActions?.()}
        <button
          type="button"
          class="btn btn-sm hover:preset-tonal text-xs"
          title={stepHint(step, "runLocally") ?? "Run locally"}
          disabled={!onRunLocally}
          onclick={() => detail && onRunLocally?.(detail, "runLocally")}
        >
          Run locally
        </button>
      {/snippet}
    </ReviewHeader>

    {#if action.error}
      <!-- Sticky: a failed write must show even with the topbar scrolled away. -->
      <div class="sticky top-0 z-20 shrink-0 bg-surface-50-950 px-5 pt-2">
        <div class="error-note rounded-md">{action.error}</div>
      </div>
    {/if}

    {#if error}
      <div class="grid flex-1 place-items-center p-8">
        <div class="error-note max-w-md">
          <div class="font-semibold">Could not load this pull request</div>
          <p class="mt-1">{error}</p>
        </div>
      </div>
    {:else}
      <div class="relative flex flex-1">
        {#if tab === "files"}
          <FileRail {files} {currentPath} {countsByPath} {viewed} onJump={jumpTo} bind:collapsedDirs />

          <section aria-label="Diffs" data-find-root class="flex min-w-0 flex-1 flex-col gap-2.5 px-5 py-4">
            {#if changes === "since" && since}
              <div class="rounded-md preset-tonal p-2 text-surface-600-400">
                {#if !since.seenSha}
                  You have not reviewed this PR yet, so this is every change.
                {:else if since.fastForward}
                  Changes pushed since your review at {shortSha(since.seenSha)}.
                {:else}
                  The git branch was rewritten since your review at {shortSha(since.seenSha)}. This compares the two
                  heads, so it can include changes from the base.
                {/if}
              </div>
            {/if}
            {#if !detail}
              <div class="placeholder h-40 animate-pulse rounded-xl"></div>
            {:else if files.length === 0}
              <p class="empty-state">No changed files.</p>
            {/if}
            {#each files as file (file.path)}
              <FileDiff
                {file}
                open={!collapsed.has(file.path)}
                viewed={viewed.has(file.path)}
                counts={countsByPath.get(file.path) ?? NO_COUNTS}
                threads={threadsByPath.get(file.path) ?? NO_THREADS}
                pending={pendingByPath.get(file.path) ?? NO_PENDING}
                {selection}
                {composer}
                menuEntries={viewed.has(file.path) ? fileMenus.viewed : fileMenus.unviewed}
                onMenu={(id) => onFileMenu(file, id)}
                onToggle={(event) => toggle(file, event)}
                onViewed={(value) => setViewed([file], value)}
                onSelectLine={(side, line, extend) => onSelectLine(file, side, line, extend)}
                onCompose={compose}
                onAskAgent={() => askAboutSelection(file)}
                onAddEntry={addEntry}
                onCancelComposer={() => (composer = null)}
                onRemoveEntry={(id) => pending.remove(id)}
                afterLine={lineAnnotations}
                annotated={annotatedLines?.get(file.path)}
                revealRow={findRow?.path === file.path ? findRow.row : null}
              >
                {#snippet thread(found)}
                  {@render threadCard(found)}
                {/snippet}
              </FileDiff>
            {/each}
          </section>
        {:else}
          <section
            aria-label={TABS.find((option) => option.value === tab)?.label}
            data-find-root
            class="min-w-0 flex-1 px-5 py-4"
          >
            {#if !detail}
              <div class="placeholder h-40 animate-pulse rounded-xl"></div>
            {:else if tab === "conversation"}
              <ConversationTab pr={detail} {conversation} {people} onJump={jumpTo} {threadCard} />
            {:else if tab === "checks"}
              <ChecksTab
                checks={detail.checks}
                logs={checkLogs}
                busy={action.busy}
                onLoadLogs={loadCheckLogs}
                onRerun={() => action.run(() => commands.reviewRerunFailedChecks(repoDir, number))}
              />
            {:else}
              {@render threadsTab?.()}
            {/if}
          </section>
        {/if}

        {#if reviewCollapsed && !reviewOpen}
          <div class="hidden w-10 shrink-0 border-l border-surface-200-800 bg-surface-50-950 @3xl/review:block">
            <div class="sticky top-0 flex flex-col items-center gap-2 py-3">
              <TooltipButton
                type="button"
                label="Expand Your review"
                class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
                aria-expanded={false}
                onclick={() => setReviewCollapsed(false)}
              >
                <PanelRightOpen size={14} aria-hidden="true" />
              </TooltipButton>
              {#if pending.entries.length > 0}
                <span class="status-badge preset-tonal-primary" title="Entries in your pending review">
                  {pending.entries.length}
                </span>
              {/if}
            </div>
          </div>
        {/if}

        <!-- Hidden rather than removed, so a half-typed summary and its cursor survive a collapse.
           The outer box holds the border at full height; the inner one stays on screen. -->
        <div
          class="w-72 shrink-0 border-l border-surface-200-800 bg-surface-50-950 @3xl/review:static @3xl/review:shadow-none {reviewOpen
            ? "absolute inset-y-0 right-0 z-10 block shadow-xl"
            : "hidden"} {reviewCollapsed && !reviewOpen ? "" : "@3xl/review:block"}"
        >
          <div class="sticky top-0 max-h-[100cqh] overflow-y-auto p-4">
            <YourReview
              {ownPr}
              {people}
              {pending}
              busy={action.busy}
              {stale}
              onSubmit={submit}
              onJump={(entry) => jumpTo(entryLocation(entry).path)}
              upNext={queue}
              {onOpenPr}
              {agentReview}
              onCollapse={() => setReviewCollapsed(true)}
            />
          </div>
        </div>
      </div>
    {/if}
  </div>

  {#if findOpen}
    <!-- Beside the Your review rail at its width, so it hides neither the rail nor its strip. -->
    <FindBar
      bind:this={findBar}
      bind:query={findQuery}
      {scroller}
      scope="{tab}:{filter}:{changes}"
      units={findUnits}
      class="right-4 {reviewCollapsed ? "@3xl/review:right-14" : "@3xl/review:right-[19rem]"}"
      onReveal={revealFindMatch}
      onClose={closeFind}
    />
  {/if}

  {#if topbarHidden}
    <!-- With the topbar gone, a way back that doesn't lose the place in the diff. -->
    <div class="absolute bottom-4 left-1/2 z-20 flex -translate-x-1/2 gap-2 whitespace-nowrap">
      {#if onBack}
        <button
          type="button"
          class="btn btn-sm preset-filled-surface-950-50 text-xs shadow-lg"
          aria-label="Back to {backLabel}"
          onclick={onBack}
        >
          <ChevronLeft size={14} aria-hidden="true" />{backLabel}
        </button>
      {/if}
      <button type="button" class="btn btn-sm preset-filled-surface-950-50 text-xs shadow-lg" onclick={scrollToTop}>
        <ArrowUp size={14} aria-hidden="true" />Back to top
      </button>
    </div>
  {/if}
</div>

{#snippet threadCard(found: CommentThread)}
  <ReviewThread
    {people}
    thread={found}
    {viewer}
    prUrl={detail?.summary.url ?? ""}
    busy={action.busy}
    onReplyNow={replyNow}
    onAddToReview={(body) => holdReply(found, body)}
    onResolve={() => resolve(found.key)}
    onEdit={editComment}
    onDelete={deleteComment}
    agentThreads={prThreads}
    onHandToAgent={(comment, askOnly, threadId) =>
      askAgent({
        threadId,
        path: found.path,
        side: (found.side as DiffSide | null) ?? "RIGHT",
        startLine: null,
        line: found.line,
        text: `${comment.author}: ${comment.body}`,
        askOnly,
      })}
    onCopy={(text) => void commands.copyText(text)}
    onOpenUrl={(url) => void commands.openExternalUrl(url)}
  />
{/snippet}
