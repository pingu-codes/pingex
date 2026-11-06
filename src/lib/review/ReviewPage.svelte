<!--
  The review page for one PR: every changed file on one scrolling page, each
  collapsible and marked viewed (per profile, by blob id); a file tree with
  comment counts; inline review threads; line and range comments and
  suggestions; and the pending review, submitted once as one GitHub review.

  Slots for agent reviews: `findingCounts` (per path) shows the Agent
  findings filter and the finding counts, `lineAnnotations` renders under
  each diff line (finding cards), `agentReview` goes in the Your review
  panel, and Findings join the review through `PendingReviewStore.add`.
-->
<script lang="ts">
  import { Check, ChevronLeft, Ellipsis, Keyboard, PanelRight } from "@lucide/svelte";
  import { Avatar } from "@skeletonlabs/skeleton-svelte";
  import { type Snippet, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { dialogStack, openDialog } from "$lib/app/dialogs.svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import { STEP_LABELS, stepHint } from "$lib/review/actions";
  import FileDiff, { type Composer, type LineSlot } from "$lib/review/FileDiff.svelte";
  import { PendingReviewStore } from "$lib/review/pendingReview.svelte";
  import RetargetDialog from "$lib/review/RetargetDialog.svelte";
  import ReviewThread, { type HandTarget } from "$lib/review/ReviewThread.svelte";
  import {
    type CommentThread,
    checksFailing,
    checksLabel,
    commentThreads,
    conversationTimeline,
    reviewVerb,
  } from "$lib/review/review";
  import {
    type AskAgentRequest,
    type DiffSide,
    entryLocation,
    fileBlob,
    fileCounts,
    fileGroups,
    fileName,
    initials,
    type LineRange,
    newSince,
    queueNeighbour,
    REVIEW_SHORTCUTS,
    type ReviewFilter,
    rangeAnchor,
    rangeText,
    reviewKey,
    selectLine,
    threadShown,
    upNext,
  } from "$lib/review/reviewPage";
  import WholeFileDialog from "$lib/review/WholeFileDialog.svelte";
  import YourReview from "$lib/review/YourReview.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { prefetch, query } from "$lib/services/queryCache.svelte";
  import { baseInput, driftLabel } from "$lib/stack/stack";
  import type {
    ChangesSince,
    CheckLog,
    CheckoutStep,
    Inbox,
    PendingEntry,
    PrComment,
    PrDetail,
    PrFile,
  } from "$lib/types";
  import { submitState } from "$lib/utils/actionState.svelte";
  import { errorMessage } from "$lib/utils/errors";

  let {
    repoDir,
    number,
    onBack,
    inbox = null,
    onOpenPr,
    onAskAgent,
    onRunLocally,
    onDetail,
    step = "noCheckout",
    seenAt = null,
    findingCounts,
    lineAnnotations,
    agentReview,
    pending: pendingReview,
    prThreads = [],
  }: {
    repoDir: string;
    number: number;
    /** The breadcrumb's way back; without it (inside the Stack page, which
     *  has its own) there is no breadcrumb. */
    onBack?: () => void;
    /** The repository's last inbox sync: the viewer, Up next and j/k. */
    inbox?: Inbox | null;
    onOpenPr?: (number: number) => void;
    onAskAgent?: (request: AskAgentRequest) => void;
    /** Run locally or Open in editor: creates a review checkout when the PR
     *  has none. Without it both are unavailable. */
    onRunLocally?: (detail: PrDetail, action: "runLocally" | "openInEditor") => void;
    /** The PR's detail, each time it loads. */
    onDetail?: (detail: PrDetail) => void;
    step?: CheckoutStep;
    /** When the reviewer last looked: the "N new since" divider. */
    seenAt?: string | null;
    /** Agent Findings per path; shows the Agent findings filter. */
    findingCounts?: Record<string, number>;
    lineAnnotations?: Snippet<[LineSlot]>;
    agentReview?: Snippet;
    /** The pending review, when the caller adds to it too (agent
     *  Findings); else the page keeps its own. */
    pending?: PendingReviewStore;
    /** Threads already on this PR: "Hand to agent" offers them. */
    prThreads?: HandTarget[];
  } = $props();

  type Tab = "conversation" | "files" | "checks" | "threads";

  /** The checkout step's dot, as the Model board draws it. */
  const STEP_DOT: Record<CheckoutStep, string> = {
    noCheckout: "border-[1.5px] border-current",
    reviewCheckout: "border-[1.5px] border-dashed border-primary-500",
    worktree: "bg-success-500",
  };

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
  let tab = $state<Tab>("files");
  let filter = $state<ReviewFilter>("all");
  const collapsed = new SvelteSet<string>();
  const viewed = new SvelteSet<string>();
  let selection = $state<LineRange | null>(null);
  let composer = $state<Composer | null>(null);
  let currentPath = $state<string | null>(null);
  let helpOpen = $state(false);
  /** Below the rail's width the Your review panel is an overlay, shown on demand. */
  let reviewOpen = $state(false);
  let changes = $state<"all" | "since">("all");
  let since = $state<ChangesSince | null>(null);
  let checkLogs = $state<CheckLog[] | null>(null);
  let lastUnresolved = -1;

  // svelte-ignore state_referenced_locally -- the page is keyed by PR.
  const pending = pendingReview ?? new PendingReviewStore(repoDir, number);
  const action = submitState();

  const viewer = $derived(inbox?.viewer ?? "");
  /** GitHub refuses Approve and Request changes from a PR's author. */
  const ownPr = $derived(!!detail && !!viewer && detail.summary.author === viewer);
  const queue = $derived(upNext(inbox, number));
  const repoName = $derived(repoDir.split(/[\\/]/).filter(Boolean).pop() ?? repoDir);
  const files = $derived<PrFile[]>(changes === "since" && since ? since.files : (detail?.files ?? []));
  const threads = $derived(commentThreads(detail?.comments ?? []));
  const shownThreads = $derived(threads.filter((thread) => threadShown(thread, filter)));
  // Shares `threads` with the Files tab, so a reply or resolve shows in both.
  const conversation = $derived(conversationTimeline(detail?.comments ?? [], detail?.reviews ?? [], threads));
  const divider = $derived(newSince(conversation, seenAt));
  const viewedCount = $derived(files.filter((file) => viewed.has(file.path)).length);
  const unresolvedCount = $derived(threads.filter((thread) => !thread.resolved).length);
  const stale = $derived(!!detail && !!pending.headSha && pending.headSha !== detail.headSha);

  let reviewStateLoaded = false;

  // Each time the detail lands (first read, a refresh, after a write).
  $effect(() => {
    const next = detail;
    if (!next) return;
    untrack(() => {
      onDetail?.(next);
      currentPath ??= next.files[0]?.path ?? null;
      if (reviewStateLoaded) return;
      reviewStateLoaded = true;
      void loadReviewState(next);
      prefetchNext();
    });
  });

  /** The stored pending review and the viewed marks, read side by side. */
  async function loadReviewState(next: PrDetail) {
    const [, marked] = await Promise.all([
      pending.loaded ? Promise.resolve() : pending.load(next.headSha),
      commands.reviewViewedFiles(repoDir, number, next.files.map(fileBlob)).catch(() => []),
    ]);
    viewed.clear();
    for (const path of marked) {
      viewed.add(path);
      collapsed.add(path);
    }
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

  // --- files ---------------------------------------------------------------

  function setOpen(path: string, open: boolean) {
    if (open) collapsed.delete(path);
    else collapsed.add(path);
  }

  function setViewed(targets: PrFile[], value: boolean) {
    for (const file of targets) {
      if (value) viewed.add(file.path);
      else viewed.delete(file.path);
      // Marking viewed collapses; unmarking opens it again.
      setOpen(file.path, !value);
    }
    if (targets.length > 0) {
      commands.reviewSetViewed(repoDir, number, targets.map(fileBlob), value).catch((cause) => {
        action.error = errorMessage(cause);
      });
    }
  }

  function toggle(file: PrFile, event: MouseEvent) {
    currentPath = file.path;
    if (event.altKey) {
      for (const other of files) setOpen(other.path, other.path === file.path);
      return;
    }
    setOpen(file.path, collapsed.has(file.path));
  }

  function jumpTo(path: string) {
    currentPath = path;
    setOpen(path, true);
    tab = "files";
    queueMicrotask(() =>
      document.querySelector(`[data-file="${CSS.escape(path)}"]`)?.scrollIntoView?.({ block: "start" }),
    );
  }

  function fileMenu(file: PrFile): MenuEntry[] {
    return [
      { id: "ask", label: "Ask agent about this file", disabled: !onAskAgent },
      { kind: "separator" },
      { id: "viewed", label: viewed.has(file.path) ? "Mark not viewed" : "Mark viewed", shortcut: "V" },
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
    currentPath = file.path;
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
        for (const other of files) setOpen(other.path, other.path === file.path);
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
      since = await commands.reviewChangesSince(repoDir, number, at.baseSha, at.headSha);
      changes = "since";
    });
  }

  // --- lines and the pending review ----------------------------------------

  function onSelectLine(file: PrFile, side: DiffSide, line: number, extend: boolean) {
    currentPath = file.path;
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

  function pendingIn(path: string): PendingEntry[] {
    // With finding cards in the diff, an added Finding shows on its card.
    return pending.entries.filter(
      (entry) => entryLocation(entry).path === path && !(lineAnnotations && entry.kind === "acceptedFinding"),
    );
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
    if (!root || !thread.key) return;
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

  const prMenu = $derived<MenuEntry[]>([
    {
      kind: "submenu",
      id: "merge",
      label: "Merge",
      entries: [
        { id: "merge:merge", label: "Create a merge commit" },
        { id: "merge:squash", label: "Squash and merge" },
        { id: "merge:rebase", label: "Rebase and merge" },
      ],
    },
    ...(detail?.summary.isDraft ? [{ id: "ready", label: "Mark ready for review" }] : []),
    { id: "retarget", label: "Retarget…" },
    { kind: "separator" },
    { id: "close", label: "Close pull request…", danger: true },
  ]);

  async function onPrMenu(id: string) {
    if (!detail) return;
    const at = detail;
    if (id.startsWith("merge:")) {
      const method = id.slice("merge:".length) as "merge" | "squash" | "rebase";
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
      const base = await openDialog(RetargetDialog, { number, base: at.summary.baseRef });
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

  // --- the keyboard --------------------------------------------------------

  function nextUnresolved() {
    const unresolved = files.flatMap((file) =>
      threads.filter((thread) => thread.path === file.path && !thread.resolved),
    );
    if (unresolved.length === 0) return;
    lastUnresolved = (lastUnresolved + 1) % unresolved.length;
    const thread = unresolved[lastUnresolved];
    jumpTo(thread.path);
    queueMicrotask(() => document.getElementById(`thread-${thread.key}`)?.scrollIntoView?.({ block: "center" }));
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
        if (next) onOpenPr?.(next.number);
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

<!-- A container: the page lays out by its own width, so it also fits beside the Stack page's tree. -->
<div class="@container/review flex h-full min-h-0 flex-col text-xs">
  <header class="flex flex-col gap-2.5 border-b border-surface-200-800 px-5 py-3">
    <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
      <!-- Where the PR is and what it is; the title truncates before the actions wrap. -->
      <div class="flex min-w-64 flex-1 items-center gap-2">
        {#if onBack}
          <button type="button" class="btn btn-sm hover:preset-tonal px-1 text-surface-500" onclick={onBack}>
            <ChevronLeft size={14} aria-hidden="true" />Pull requests
          </button>
          <span class="text-surface-500">/</span>
          <span class="text-surface-500">{repoName}</span>
          <span class="text-surface-500">/</span>
        {/if}
        {#if detail}
          <h1 class="min-w-0 truncate text-sm leading-5 font-semibold" title={detail.summary.title}>
            {detail.summary.title}
          </h1>
          <span class="font-mono text-surface-500">#{number}</span>
          <Avatar class="avatar"><Avatar.Fallback>{initials(detail.summary.author)}</Avatar.Fallback></Avatar>
          <span class="whitespace-nowrap text-surface-500">{detail.summary.author}</span>
          {#if detail.summary.isDraft}
            <span class="status-badge preset-tonal">Draft</span>
          {/if}
          {#if drift}
            <span
              class="status-badge preset-tonal-warning whitespace-nowrap"
              title="The base has commits this PR does not"
              >{drift}</span
            >
          {/if}
        {:else}
          <span class="placeholder h-4 w-48 animate-pulse rounded"></span>
        {/if}
      </div>
      <div class="ml-auto flex shrink-0 items-center gap-2">
        <span
          class="inline-flex items-center gap-1.5 rounded-full border border-surface-300-700 px-2 py-0.5 text-[11px]"
          title="Code for this PR on disk"
        >
          <span class="size-2 shrink-0 rounded-full {STEP_DOT[step]}" aria-hidden="true"></span>{STEP_LABELS[step]}
        </span>
        <button
          type="button"
          class="btn btn-sm hover:preset-tonal text-xs"
          title={stepHint(step, "runLocally") ?? "Run locally"}
          disabled={!onRunLocally}
          onclick={() => detail && onRunLocally?.(detail, "runLocally")}
        >
          Run locally
        </button>
        <button
          type="button"
          class="btn btn-sm hover:preset-tonal text-xs"
          disabled={!detail}
          onclick={() => detail && commands.openExternalUrl(detail.summary.url)}
        >
          Open on GitHub
        </button>
        <ActionMenu
          entries={prMenu}
          label="Pull request {number}"
          onSelect={onPrMenu}
          buttonLabel="Pull request actions"
        >
          {#snippet row({ buttonProps })}
            <button
              {...buttonProps}
              type="button"
              class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
              disabled={!detail}
            >
              <Ellipsis size={14} aria-hidden="true" />
            </button>
          {/snippet}
        </ActionMenu>
      </div>
    </div>
    <div class="flex flex-wrap items-center gap-2">
      <Segmented label="Pull request" value={tab} options={TABS} onChange={(value) => (tab = value)} />
      {#if tab === "files"}
        {#each FILTERS as option (option.value)}
          <button
            type="button"
            class={filter === option.value ? "pill-active" : "pill"}
            aria-pressed={filter === option.value}
            onclick={() => (filter = option.value)}
          >
            {option.label}
          </button>
        {/each}
        <Segmented
          label="Changes"
          value={changes}
          options={[
            { value: "all", label: "All changes" },
            { value: "since", label: "Since your review" },
          ]}
          onChange={showChanges}
        />
        <span class="grow"></span>
        <span class="text-surface-500">Viewed {viewedCount} of {files.length}</span>
        <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={() => collapsed.clear()}>
          Expand all
        </button>
        <button
          type="button"
          class="btn btn-sm hover:preset-tonal text-[11px]"
          onclick={() => {
            for (const file of files) collapsed.add(file.path);
          }}
        >
          Collapse all
        </button>
      {:else}
        <span class="grow"></span>
      {/if}
      <button
        type="button"
        class="btn btn-sm text-[11px] @3xl/review:hidden {reviewOpen ? "preset-tonal" : "hover:preset-tonal"}"
        aria-expanded={reviewOpen}
        onclick={() => (reviewOpen = !reviewOpen)}
      >
        <PanelRight size={14} aria-hidden="true" />Your review{pending.entries.length > 0
          ? ` · ${pending.entries.length}`
          : ""}
      </button>
      <button
        type="button"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
        aria-label="Shortcuts"
        aria-expanded={helpOpen}
        onclick={() => (helpOpen = !helpOpen)}
      >
        <Keyboard size={14} aria-hidden="true" />
      </button>
    </div>
    {#if helpOpen}
      <div
        class="popover-card grid max-w-md grid-cols-2 gap-x-4 gap-y-1.5 self-end p-3"
        role="dialog"
        aria-label="Shortcuts"
      >
        {#each REVIEW_SHORTCUTS as shortcut (shortcut.label)}
          <div class="flex justify-between gap-2">
            <span>{shortcut.label}</span>
            <span class="flex gap-1"
              >{#each shortcut.keys as key (key)}
                <kbd class="kbd">{key}</kbd>
              {/each}</span
            >
          </div>
        {/each}
      </div>
    {/if}
  </header>

  {#if action.error}
    <div class="error-note mx-5 mt-2 rounded-md">{action.error}</div>
  {/if}

  {#if error}
    <div class="grid flex-1 place-items-center p-8">
      <div class="error-note max-w-md">
        <div class="font-semibold">Could not load this pull request</div>
        <p class="mt-1">{error}</p>
      </div>
    </div>
  {:else}
    <div class="relative flex min-h-0 flex-1">
      {#if tab === "files"}
        <nav
          aria-label="Changed files"
          class="hidden w-56 shrink-0 @5xl/review:flex flex-col gap-0.5 overflow-y-auto border-r border-surface-200-800 bg-surface-100-900 px-2 py-3"
        >
          {#each fileGroups(files) as group (group.dir)}
            {#if group.dir}
              <div class="eyebrow px-2 pt-2 pb-1.5">{group.dir}</div>
            {/if}
            {#each group.files as file (file.path)}
              {@const counts = fileCounts(threads, file.path, findingCounts?.[file.path] ?? 0)}
              <button
                type="button"
                class="menu-row {currentPath === file.path ? "bg-surface-200-800" : ""}"
                aria-current={currentPath === file.path}
                onclick={() => jumpTo(file.path)}
              >
                <span class="min-w-0 grow truncate font-mono text-[11px]">{fileName(file.path)}</span>
                {#if counts.findings > 0}
                  <span class="status-badge preset-tonal-primary" title="Agent findings">{counts.findings}</span>
                {/if}
                {#if counts.comments > 0}
                  <span class="status-badge preset-tonal" title="Comments">{counts.comments}</span>
                {/if}
                {#if viewed.has(file.path)}
                  <Check size={12} class="text-success-500" aria-label="Viewed" />
                {/if}
              </button>
            {/each}
          {/each}
          <div class="grow"></div>
          <div class="flex flex-col gap-1.5 border-t border-surface-200-800 p-2 text-surface-500">
            <div class="flex items-center gap-1.5">
              <kbd class="kbd">]</kbd><kbd class="kbd">[</kbd>Next / previous file
            </div>
            <div class="flex items-center gap-1.5"><kbd class="kbd">v</kbd>Mark viewed and collapse</div>
          </div>
        </nav>

        <section aria-label="Diffs" class="flex min-w-0 flex-1 flex-col gap-2.5 overflow-y-auto px-5 py-4">
          {#if changes === "since" && since}
            <div class="rounded-md preset-tonal p-2 text-surface-600-400">
              {#if !since.seenSha}
                You have not reviewed this PR yet, so this is every change.
              {:else if since.fastForward}
                Changes pushed since your review at {since.seenSha.slice(0, 7)}.
              {:else}
                The git branch was rewritten since your review at {since.seenSha.slice(0, 7)}. This compares the two
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
              counts={fileCounts(threads, file.path, findingCounts?.[file.path] ?? 0)}
              threads={shownThreads.filter((thread) => thread.path === file.path)}
              pending={pendingIn(file.path)}
              {selection}
              {composer}
              menuEntries={fileMenu(file)}
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
          class="min-w-0 flex-1 overflow-y-auto px-5 py-4"
        >
          {#if !detail}
            <div class="placeholder h-40 animate-pulse rounded-xl"></div>
          {:else if tab === "conversation"}
            {@render conversationTab(detail)}
          {:else if tab === "checks"}
            {@render checksTab(detail)}
          {:else}
            <p class="empty-state">Threads started on this PR show here, with their git branch and commit.</p>
          {/if}
        </section>
      {/if}

      <div
        class="w-72 shrink-0 overflow-y-auto border-l border-surface-200-800 bg-surface-50-950 p-4 @3xl/review:static @3xl/review:block @3xl/review:shadow-none {reviewOpen
          ? "absolute inset-y-0 right-0 z-10 block shadow-xl"
          : "hidden"}"
      >
        <YourReview
          {ownPr}
          {pending}
          busy={action.busy}
          {stale}
          onSubmit={submit}
          onJump={(entry) => jumpTo(entryLocation(entry).path)}
          upNext={queue}
          {onOpenPr}
          {agentReview}
        />
      </div>
    </div>
  {/if}
</div>

{#snippet threadCard(found: CommentThread)}
  <ReviewThread
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

{#snippet conversationThread(found: CommentThread)}
  <!-- Away from the diff, a thread says where it is; the link opens the file. -->
  <div class="flex flex-col gap-1">
    <button
      type="button"
      class="self-start font-mono text-[11px] text-surface-500 hover:underline"
      onclick={() => jumpTo(found.path)}
    >
      {found.path}:{found.line ?? "file"}
    </button>
    {@render threadCard(found)}
  </div>
{/snippet}

{#snippet conversationTab(pr: PrDetail)}
  <div class="flex max-w-3xl flex-col gap-3">
    {#if pr.body.trim()}
      <div class="panel-card p-3 text-[13px] leading-5 whitespace-pre-wrap">{pr.body}</div>
    {/if}
    {#each conversation as entry, index (entry.key)}
      {#if divider && index === divider.index}
        <div class="flex items-center gap-2 text-primary-500" role="separator">
          <span class="h-px grow bg-primary-500"></span>
          <span class="text-[10px] font-semibold">
            {divider.count}
            new since {new Date(seenAt ?? "").toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}
          </span>
          <span class="h-px grow bg-primary-500"></span>
        </div>
      {/if}
      {#if entry.kind === "comment"}
        <div class="panel-card flex flex-col gap-1 p-2.5">
          <div class="flex items-center gap-1.5">
            <Avatar class="avatar"><Avatar.Fallback>{initials(entry.comment.author)}</Avatar.Fallback></Avatar>
            <b class="font-medium">{entry.comment.author}</b>
            <span class="text-surface-500">{new Date(entry.comment.createdAt).toLocaleString()}</span>
          </div>
          <div class="pl-6 text-[13px] leading-5 whitespace-pre-wrap">{entry.comment.body}</div>
        </div>
      {:else if entry.kind === "review"}
        {@const review = entry.review}
        <div class="panel-card flex flex-col gap-2 p-2.5">
          <div class="flex items-center gap-1.5">
            <Avatar class="avatar"><Avatar.Fallback>{initials(review.author)}</Avatar.Fallback></Avatar>
            <b class="font-medium">{review.author}</b>
            {#if review.state === "APPROVED"}
              <span class="status-badge preset-tonal-success">{reviewVerb(review.state)}</span>
            {:else if review.state === "CHANGES_REQUESTED"}
              <span class="status-badge preset-tonal-error">{reviewVerb(review.state)}</span>
            {:else}
              <span class="text-surface-500">{reviewVerb(review.state)}</span>
            {/if}
            <span class="text-surface-500">{new Date(review.submittedAt).toLocaleString()}</span>
          </div>
          {#if review.body.trim()}
            <div class="pl-6 text-[13px] leading-5 whitespace-pre-wrap">{review.body}</div>
          {/if}
          {#each entry.threads as found (found.key)}
            {@render conversationThread(found)}
          {/each}
        </div>
      {:else}
        {@render conversationThread(entry.thread)}
      {/if}
    {:else}
      <p class="empty-state">No conversation yet.</p>
    {/each}
    {#if pr.commits.length > 0}
      <div class="eyebrow mt-2">Commits</div>
      <div class="panel-card flex flex-col p-1.5">
        {#each pr.commits as commit (commit.oid)}
          <div class="menu-row">
            <span class="font-mono text-[11px] text-surface-500">{commit.shortOid}</span>{commit.headline}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#snippet checksTab(pr: PrDetail)}
  <div class="flex max-w-3xl flex-col gap-3">
    <div class="flex items-center gap-2">
      <span class={checksFailing(pr.checks) ? "text-error-500" : "text-surface-500"}>
        {checksLabel(pr.checks) ?? "No checks on this head."}
      </span>
      <span class="grow"></span>
      {#if checksFailing(pr.checks)}
        <button
          type="button"
          class="btn btn-sm hover:preset-tonal text-[11px]"
          disabled={action.busy}
          onclick={loadCheckLogs}
        >
          View failing logs
        </button>
        <button
          type="button"
          class="btn btn-sm preset-tonal text-[11px]"
          disabled={action.busy}
          onclick={() => action.run(() => commands.reviewRerunFailedChecks(repoDir, number))}
        >
          Re-run failed checks
        </button>
      {/if}
    </div>
    {#each checkLogs ?? [] as log (log.name)}
      <div class="panel-card overflow-hidden">
        <div class="file-head font-mono text-[11px] font-semibold">{log.name}</div>
        <pre class="max-h-80 overflow-auto p-2 font-mono text-[11px] whitespace-pre-wrap">{log.log}</pre>
      </div>
    {/each}
  </div>
{/snippet}
