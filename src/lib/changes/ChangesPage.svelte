<!--
  A thread's Changes page: what the agent changed, the way a pull request
  shows it. Each repository the thread works in is split into Change segments
  (stack PRs, git branches, the working tree); choosing one lays its files out
  in a tree beside their diffs. "This thread" narrows it to what moved since
  the thread's start tips. It stays current while the agent works, and notes
  left on lines go back to the agent as one message.
-->
<script lang="ts">
  import { ChevronLeft, MessageSquareText, RefreshCw, Send, X } from "@lucide/svelte";
  import { onDestroy, untrack } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { place } from "$lib/app/places";
  import ChangesRail from "$lib/changes/ChangesRail.svelte";
  import {
    allSegments,
    pickSegment,
    RefreshScheduler,
    segmentFiles,
    segmentKey,
    totals,
    viewedKey,
  } from "$lib/changes/changesPage";
  import { type ChangeNote, noteEntry, notesMessage } from "$lib/changes/notes";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import DiffBlock from "$lib/components/DiffBlock.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import { DiffReading } from "$lib/review/diffReading.svelte";
  import FileDiff from "$lib/review/FileDiff.svelte";
  import FileRail from "$lib/review/FileRail.svelte";
  import { type FileCounts, type LineRange, rangeText } from "$lib/review/reviewPage";
  import { activeTurns } from "$lib/services/codexEvents.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { invalidate, query } from "$lib/services/queryCache.svelte";
  import { collectFileChanges } from "$lib/thread/fileChanges";
  import { openSession, releaseSession } from "$lib/thread/sessions.svelte";
  import { toaster, toastFailure } from "$lib/toaster";
  import type { ChangeSegment, ChangesScope, PrFile, RepoChanges } from "$lib/types";
  import { plural } from "$lib/utils/format";

  /** How often a running agent's repositories are read, besides its own edits. */
  const POLL_MS = 5000;
  const NO_COUNTS: FileCounts = { comments: 0, unresolved: 0, findings: 0 };

  let {
    threadId,
    cwd,
    title,
    initialPath = null,
    initialSegment = null,
    onBack,
    backLabel,
    onOpenThread,
  }: {
    threadId: string;
    /** The thread's directory as the sidebar knows it. */
    cwd: string | null;
    title: string;
    initialPath?: string | null;
    initialSegment?: string | null;
    onBack: () => void;
    backLabel: string;
    onOpenThread: () => void;
  } = $props();

  // What the page had open, kept for when it is reached again.
  const left = untrack(() =>
    place(`changes:${threadId}`, () => ({
      scope: "branch" as ChangesScope,
      segment: null as string | null,
      collapsedDirs: [] as string[],
      viewed: [] as string[],
      collapsed: [] as string[],
      notes: [] as ChangeNote[],
    })),
  );
  let scope = $state<ChangesScope>(left.scope);
  let wanted = $state<string | null>(untrack(() => initialSegment) ?? left.segment);
  let collapsedDirs = $state<string[]>(left.collapsedDirs);
  const viewed = new SvelteSet<string>(left.viewed);
  const collapsed = new SvelteSet<string>(left.collapsed);
  let notes = $state<ChangeNote[]>(left.notes);
  $effect(() => {
    left.scope = scope;
    left.collapsedDirs = collapsedDirs;
    left.viewed = [...viewed];
    left.collapsed = [...collapsed];
    left.notes = notes;
  });

  // The session gives live items (to refresh on each edit) and `send`.
  const session = untrack(() => openSession(threadId));
  onDestroy(() => releaseSession(session));
  const running = $derived(activeTurns.list.includes(threadId) || session.activeTurn !== null);

  const overviewQuery = query("gitThreadChanges", () => [threadId, cwd, scope]);
  const overview = $derived(overviewQuery.value ?? null);
  const selected = $derived(
    pickSegment(
      overview,
      wanted,
      untrack(() => initialPath),
    ),
  );
  const selectedKey = $derived(selected ? segmentKey(selected.repo.root, selected.segment.id) : null);
  $effect(() => {
    if (selectedKey) left.segment = selectedKey;
  });

  const repo = $derived<RepoChanges | null>(selected?.repo ?? null);
  const segment = $derived<ChangeSegment | null>(selected?.segment ?? null);
  const working = $derived(segment?.kind === "uncommitted");
  const committedQuery = query("gitSegmentDiff", () =>
    repo && segment?.headSha && !working && segment.files.length > 0 && segment.baseSha
      ? [repo.root, segment.baseSha, segment.headSha]
      : null,
  );
  const workingQuery = query("gitWorkingDiff", () =>
    repo && segment && working ? [repo.root, segment.baseSha] : null,
  );
  const diffQuery = $derived(working ? workingQuery : committedQuery);
  const diff = $derived(diffQuery.value ?? null);
  const files = $derived<PrFile[]>(segment ? segmentFiles(segment, diff?.files) : []);
  const listed = $derived(new Map((segment?.files ?? []).map((file) => [file.path, file])));
  const summary = $derived(totals(overview));

  /** The working tree's patches change without its SHAs changing: read them
   *  again only when the listing says a file did. */
  let lastSignature: string | null = null;
  $effect(() => {
    if (!repo || !segment || !working) {
      lastSignature = null;
      return;
    }
    const signature = `${repo.root}\u0000${segment.files.map((file) => `${file.path}:${file.contentKey}`).join("\u0000")}`;
    if (lastSignature !== null && lastSignature !== signature) {
      untrack(() => invalidate({ names: ["gitWorkingDiff"], repo: repo.root }));
    }
    lastSignature = signature;
  });

  // --- live -------------------------------------------------------------------

  const scheduler = new RefreshScheduler(() => invalidate({ names: ["gitThreadChanges"] }));
  onDestroy(() => scheduler.dispose());

  /** Edits and commands the running turn has finished: each may have changed files. */
  const finishedWork = $derived(
    (session.activeTurn?.items ?? []).filter(
      (item) => (item.type === "fileChange" || item.type === "commandExecution") && item.status !== "inProgress",
    ).length,
  );
  $effect(() => {
    if (finishedWork > 0) untrack(() => scheduler.poke());
  });

  // Subagents, background processes and edits no item reports: a slow poll,
  // only while the agent runs and the window is showing.
  let visible = $state(typeof document === "undefined" || document.visibilityState === "visible");
  $effect(() => {
    if (!running || !visible) return;
    const timer = setInterval(() => scheduler.flush(), POLL_MS);
    return () => clearInterval(timer);
  });

  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 5000);
    return () => clearInterval(timer);
  });
  const updated = $derived.by(() => {
    if (!overview) return null;
    const seconds = Math.max(0, Math.round((now - overview.readAt) / 1000));
    return seconds < 5
      ? "Updated just now"
      : seconds < 60
        ? `Updated ${seconds}s ago`
        : `Updated ${Math.round(seconds / 60)}m ago`;
  });

  function refresh() {
    invalidate({ names: ["gitThreadChanges", "gitWorkingDiff"] });
  }

  // Without a repository, the edits the agent reported are all there is.
  const reported = $derived(
    overview && overview.repos.length === 0
      ? collectFileChanges((session.thread?.turns ?? []).flatMap((turn) => turn.items))
      : [],
  );

  // --- files ------------------------------------------------------------------

  function fileId(path: string): string {
    return selectedKey ? `${selectedKey}\u0000${path}` : path;
  }

  function isViewed(path: string): boolean {
    const file = listed.get(path);
    return !!(file && repo && segment && viewed.has(viewedKey(repo.root, segment, file)));
  }

  const viewedPaths = $derived(new Set(files.filter((file) => isViewed(file.path)).map((file) => file.path)));

  // Folds and viewed marks are the page's, per segment; a note on lines waits
  // to go to the agent.
  const reading = new DiffReading({
    files: () => files,
    folds: {
      has: (path) => collapsed.has(fileId(path)),
      add: (path) => collapsed.add(fileId(path)),
      delete: (path) => collapsed.delete(fileId(path)),
    },
    viewed: {
      has: isViewed,
      set(targets, value) {
        if (!repo || !segment) return;
        for (const target of targets) {
          const file = listed.get(target.path);
          if (!file) continue;
          const key = viewedKey(repo.root, segment, file);
          if (value) viewed.add(key);
          else viewed.delete(key);
        }
      },
    },
    addEntry: addNote,
  });

  // Opened on a file: go to it once its segment's files are on the page.
  let jumped = false;
  $effect(() => {
    const path = untrack(() => initialPath);
    if (jumped || !path || !files.some((file) => file.path === path)) return;
    jumped = true;
    untrack(() => reading.jumpTo(path));
  });

  function absolute(path: string): string {
    if (!repo) return path;
    const separator = repo.root.includes("\\") && !repo.root.includes("/") ? "\\" : "/";
    return `${repo.root.replace(/[\\/]+$/, "")}${separator}${path}`;
  }

  const fileMenus = $derived({ viewed: fileMenu(true), unviewed: fileMenu(false) });

  function fileMenu(isFileViewed: boolean): MenuEntry[] {
    return [
      { id: "viewed", label: isFileViewed ? "Mark not viewed" : "Mark viewed" },
      { id: "others", label: "Collapse all others", hint: "⌥click" },
      { kind: "separator" },
      { id: "editor", label: "Open in editor" },
      { id: "reveal", label: "Reveal in Finder" },
      { id: "copy", label: "Copy path" },
    ];
  }

  function onFileMenu(file: PrFile, id: string) {
    reading.hold(file.path);
    switch (id) {
      case "viewed":
        reading.setViewed([file], !isViewed(file.path));
        break;
      case "others":
        reading.toggle(file.path, true);
        break;
      case "editor":
        commands.openInZed(absolute(file.path)).catch(toastFailure);
        break;
      case "reveal":
        commands.revealInFinder(absolute(file.path)).catch(toastFailure);
        break;
      case "copy":
        void commands.copyText(file.path);
        break;
    }
  }

  // --- notes ------------------------------------------------------------------

  function segmentWords(at: ChangeSegment): string {
    if (at.kind === "uncommitted") return "uncommitted changes";
    const branch = at.branch ? `branch \`${at.branch}\`` : at.label;
    return at.prNumber !== null ? `${branch} (#${at.prNumber})` : branch;
  }

  function addNote(range: LineRange, body: string, replacement: string | null) {
    if (!repo || !segment) return;
    const file = files.find((candidate) => candidate.path === range.path);
    notes = [
      ...notes,
      {
        id: crypto.randomUUID(),
        root: repo.root,
        repoLabel: repo.label,
        segmentId: segment.id,
        segmentLabel: segmentWords(segment),
        path: range.path,
        side: range.side,
        startLine: range.start < range.end ? range.start : null,
        line: range.end,
        excerpt: file ? rangeText(file, range) : "",
        body,
        replacement,
      },
    ];
  }

  function removeNote(id: string) {
    notes = notes.filter((note) => note.id !== id);
  }

  /** The notes FileDiff draws, for the segment on screen. */
  const notesByPath = $derived.by(() => {
    const byPath = new Map<string, ReturnType<typeof noteEntry>[]>();
    if (!repo || !segment) return byPath;
    for (const note of notes) {
      if (note.root !== repo.root || note.segmentId !== segment.id) continue;
      byPath.set(note.path, [...(byPath.get(note.path) ?? []), noteEntry(note)]);
    }
    return byPath;
  });

  let sending = $state(false);
  async function sendNotes() {
    if (notes.length === 0 || sending) return;
    sending = true;
    const busy = running;
    try {
      const sent = await session.send([{ type: "text", text: notesMessage(notes) }]);
      if (!sent) {
        toastFailure("The notes could not be sent. They are still here.");
        return;
      }
      notes = [];
      toaster.create({
        type: "info",
        title: busy ? "Notes queued" : "Notes sent",
        description: busy ? "The agent gets them when it finishes this turn." : undefined,
        closable: true,
        action: { label: "Open thread", onClick: onOpenThread },
      });
    } catch (cause) {
      toastFailure(cause);
    } finally {
      sending = false;
    }
  }

  function walkSegments(step: 1 | -1) {
    const segments = allSegments(overview);
    const at = segments.findIndex((ref) => segmentKey(ref.repo.root, ref.segment.id) === selectedKey);
    const next = segments[at + step];
    if (next) wanted = segmentKey(next.repo.root, next.segment.id);
  }

  /** The review page's keys, where they mean something here: j/k walk the
   *  segments and ⌘↵ sends; [ and ], v, c and Escape are the reading's. */
  const pageKeys = {
    nextPr: () => walkSegments(1),
    previousPr: () => walkSegments(-1),
    submit: () => void sendNotes(),
  };

  const scopes = [
    { value: "branch" as const, label: "Branch", title: "Each git branch against its base, as its PR shows it" },
    { value: "thread" as const, label: "This thread", title: "Only what moved since this thread started" },
  ];
</script>

<div class="relative h-full min-h-0 text-xs [container:review/size]">
  <div {@attach reading.attach} data-diff-pane class="flex h-full flex-col overflow-x-hidden overflow-y-auto">
    <header class="flex shrink-0 flex-col gap-2.5 border-b border-surface-200-800 px-5 py-3">
      <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
        <div class="flex min-w-64 flex-1 items-center gap-2">
          <button type="button" class="btn btn-sm hover:preset-tonal px-1 text-surface-500" onclick={onBack}>
            <ChevronLeft size={14} aria-hidden="true" />{backLabel}
          </button>
          <span class="text-surface-500">/</span>
          <h1 class="text-sm leading-5 font-semibold">Changes</h1>
          <button
            type="button"
            class="min-w-0 truncate text-surface-500 hover:underline"
            title="Open the thread"
            onclick={onOpenThread}
          >
            {title}
          </button>
        </div>
        <div class="ml-auto flex shrink-0 items-center gap-2">
          {#if running}
            <span class="status-badge preset-tonal-primary gap-1.5" aria-live="polite">
              <span class="size-1.5 animate-pulse rounded-full bg-primary-500"></span>Agent working
            </span>
          {:else if updated}
            <span class="text-surface-500">{updated}</span>
          {/if}
          <button
            type="button"
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
            aria-label="Refresh"
            title="Refresh"
            onclick={refresh}
          >
            <RefreshCw size={14} class={overviewQuery.refreshing ? "animate-spin" : ""} aria-hidden="true" />
          </button>
          {#if notes.length > 0}
            <button type="button" class="btn btn-sm hover:preset-tonal text-surface-500" onclick={() => (notes = [])}>
              <X size={13} aria-hidden="true" />Discard
            </button>
          {/if}
          <button
            type="button"
            class="btn btn-sm preset-filled-primary-500"
            disabled={notes.length === 0 || sending}
            onclick={sendNotes}
            title={notes.length === 0 ? "Select lines in a diff to leave a note" : undefined}
          >
            <Send size={13} aria-hidden="true" />Send {notes.length > 0 ? plural(notes.length, "note") : "notes"} to
            agent
          </button>
        </div>
      </div>
      <div class="flex flex-wrap items-center gap-2">
        <Segmented label="Show changes" value={scope} options={scopes} onChange={(value) => (scope = value)} />
        <span class="text-surface-500">
          {#if overview}
            {plural(summary.files, "file")}
            ·
            <span class="text-success-600-400">+{summary.additions}</span>
            <span class="text-error-600-400">−{summary.deletions}</span>
          {/if}
        </span>
        <span class="grow"></span>
        {#if notes.length > 0}
          <span class="flex items-center gap-1 text-surface-500">
            <MessageSquareText size={13} aria-hidden="true" />{plural(notes.length, "note")}
            waiting
          </span>
        {/if}
      </div>
    </header>

    {#if overviewQuery.error && !overview}
      <div class="grid flex-1 place-items-center p-8">
        <div class="error-note max-w-md">
          <div class="font-semibold">Could not read this thread's changes</div>
          <p class="mt-1">{overviewQuery.error}</p>
        </div>
      </div>
    {:else if !overview}
      <div class="space-y-3 p-5">
        <div class="placeholder h-10 animate-pulse rounded-xl"></div>
        <div class="placeholder h-40 animate-pulse rounded-xl opacity-70"></div>
      </div>
    {:else if overview.repos.length === 0}
      <section aria-label="Reported edits" class="mx-auto w-full max-w-4xl space-y-3 px-5 py-4">
        <p class="text-surface-500">
          This thread's folder is not in a Git repository, so these are the edits the agent reported.
        </p>
        {#each reported as change (change.path)}
          <DiffBlock {change} />
        {:else}
          <p class="empty-state">No file changes in this thread.</p>
        {/each}
      </section>
    {:else}
      <div class="relative flex flex-1">
        <ChangesRail repos={overview.repos} {selectedKey} onSelect={(key) => (wanted = key)} />
        {#if segment && files.length > 0}
          <FileRail
            {files}
            currentPath={reading.currentPath}
            countsByPath={new Map()}
            viewed={viewedPaths}
            onJump={(path) => reading.jumpTo(path)}
            bind:collapsedDirs
          />
        {/if}
        <section aria-label="Diffs" class="flex min-w-0 flex-1 flex-col gap-2.5 px-5 py-4">
          {#if segment && repo}
            <div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
              <h2 class="text-sm font-semibold">{segment.label}</h2>
              <span class="text-surface-500">
                {repo.label}
                · compared with <span class="font-mono">{segment.baseLabel}</span>
              </span>
            </div>
            {#if segment.needsRestack}
              <div class="rounded-md preset-tonal-warning p-2">
                The PR below moved and this git branch is not on top of it, so this can include changes from the old
                version of the PR below. Restack the stack to fix it.
              </div>
            {:else if segment.rewritten}
              <div class="rounded-md preset-tonal-warning p-2">
                The git branch was rewritten since the thread started. This compares the two tips, so it can include
                changes from the base.
              </div>
            {/if}
            {#if segment.note}
              <p class="text-surface-500">{segment.note}</p>
            {/if}
            {#if diffQuery.error}
              <div class="error-note">{diffQuery.error}</div>
            {/if}
            {#if segment.truncated || diff?.truncated}
              <p class="text-surface-500">
                Showing the first {segment.files.length} of {segment.totalFiles} files; the rest are too many to read.
              </p>
            {/if}
            {#each files as file (file.path)}
              {@const fileViewed = viewedPaths.has(file.path)}
              <FileDiff
                {file}
                open={reading.isOpen(file.path)}
                viewed={fileViewed}
                counts={NO_COUNTS}
                threads={[]}
                pending={notesByPath.get(file.path) ?? []}
                selection={reading.selection}
                composer={reading.composer}
                menuEntries={fileViewed ? fileMenus.viewed : fileMenus.unviewed}
                onMenu={(id) => onFileMenu(file, id)}
                onToggle={(event) => reading.toggle(file.path, event.altKey)}
                onViewed={(value) => reading.setViewed([file], value)}
                onSelectLine={(side, line, extend) => reading.selectLine(file.path, side, line, extend)}
                onCompose={(mode) => reading.compose(mode)}
                addLabel="Add note"
                onAddEntry={(body, replacement) => reading.add(body, replacement)}
                onCancelComposer={() => reading.compose(null)}
                onRemoveEntry={removeNote}
              />
            {:else}
              <p class="empty-state">{segment.note ?? "No changed files."}</p>
            {/each}
          {:else}
            <p class="empty-state">
              {overview.repos.find((candidate) => candidate.note)?.note ??
                (scope === "thread" ? "Nothing has changed since this thread started." : "No changes.")}
            </p>
          {/if}
        </section>
      </div>
    {/if}
  </div>
</div>

<svelte:window onkeydown={(event) => reading.onKeydown(event, pageKeys)} />
<svelte:document onvisibilitychange={() => (visible = document.visibilityState === "visible")} />
