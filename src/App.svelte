<script lang="ts">
import { ArrowLeft, RefreshCw, X } from "@lucide/svelte";
import {
  askCodexReview,
  menuAction,
  openSubagent,
  openWorkspaceDialog,
  renameProjectAt,
  slashCommand,
  threadCreated,
} from "$lib/app/actions.svelte";
import { addProject, appData, applyData, projects, refresh } from "$lib/app/appData.svelte";
import { expectedCwdFor, handoff, movedToWorktree } from "$lib/app/handoff.svelte";
import {
  browseForHome,
  chooseHome,
  codexBinary,
  codexHome,
  init,
  launch,
  removeHome,
  setBinary,
  switchHome,
} from "$lib/app/launch.svelte";
import { startApp } from "$lib/app/listeners.svelte";
import {
  currentProject,
  detailProject,
  focusProjectPath,
  goHome,
  newThread,
  newThreadInDir,
  openParentThread,
  openProjectDetail,
  openReview,
  openThread,
  openThreadById,
  openThreadInCwd,
  openWorktrees,
  reviewRepo,
  selectedThreadInfo,
  view,
  worktreesRepo,
} from "$lib/app/navigation.svelte";
import DialogHost from "$lib/components/DialogHost.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import HomePage from "$lib/layout/HomePage.svelte";
import HomePicker from "$lib/layout/HomePicker.svelte";
import SettingsView from "$lib/layout/SettingsView.svelte";
import Sidebar from "$lib/layout/Sidebar.svelte";
import ProjectDetail from "$lib/panels/ProjectDetail.svelte";
import ReviewView from "$lib/review/ReviewView.svelte";
import { openHomeWindow, revealInFinder } from "$lib/services/api";
import { closeSettings, openSettings, settingsNav } from "$lib/services/settingsNav.svelte";
import ToastHost from "$lib/ToastHost.svelte";
import ThreadDebugPopover from "$lib/thread/ThreadDebugPopover.svelte";
import ThreadHeader from "$lib/thread/ThreadHeader.svelte";
import ThreadView from "$lib/thread/ThreadView.svelte";
import { dragRegion } from "$lib/utils/dragRegion";
import { loadSize, resizeHandle } from "$lib/utils/resize";
import Worktrees from "$lib/worktrees/Worktrees.svelte";

let settingsOpen = $state(false);
let sidebarWidth = $state(loadSize("layout.sidebarWidth", 280, 200, 480));

const thread = $derived(selectedThreadInfo());
const project = $derived(currentProject());
const review = $derived(reviewRepo());
const worktrees = $derived(worktreesRepo());
const detail = $derived(detailProject());

// Deep links (e.g. an MCP tool-call in the thread view) request Settings via
// the shared settingsNav store; mirror that into the local open state.
$effect(() => {
  void settingsNav.nonce;
  if (settingsNav.open) settingsOpen = true;
});

startApp();
init();
</script>

{#if launch.phase === "picker" && launch.state}
  <div class="h-screen min-h-[560px]" data-tauri-drag-region use:dragRegion>
    <HomePicker
      launchState={launch.state}
      busy={launch.busy}
      error={launch.error}
      onSelect={chooseHome}
      onBrowse={browseForHome}
      onRemove={removeHome}
      onSetBinary={setBinary}
      onOpenNewWindow={(path) => void openHomeWindow(path)}
    />
  </div>
{:else if launch.phase === "loading"}
  <div class="grid h-screen min-h-[560px] place-items-center bg-surface-50-950">
    <RefreshCw size={20} class="animate-spin text-surface-500" />
  </div>
{:else}
<main class="flex h-screen min-h-[560px] overflow-hidden bg-surface-50-950 text-surface-950-50">
  <div class="relative h-full shrink-0" style="width: {sidebarWidth}px">
  <Sidebar
    projects={projects()}
    account={appData.data?.account ?? null}
    sideQuestions={appData.data?.sideQuestions ?? []}
    sections={appData.data?.sections ?? []}
    sectionsSupported={appData.data?.sectionsSupported ?? false}
    selectedThread={view.threadId}
    loading={appData.loading}
    onAddProject={addProject}
    onAddWorkspace={() => openWorkspaceDialog()}
    onSelectThread={openThread}
    onNewThread={newThread}
    onGoHome={goHome}
    onOpenSettings={() => openSettings("general")}
    onMenuAction={menuAction}
    onSelectArchived={(archived) => openThreadInCwd(archived.id, archived.cwd)}
    onUnarchived={applyData}
    onOpenWorktrees={openWorktrees}
    currentProject={project}
    onOpenSearchResult={(item) => openThreadInCwd(item.id, item.cwd)}
  />
  <div
    role="separator"
    aria-orientation="vertical"
    aria-label="Resize sidebar"
    class="absolute inset-y-0 -right-1 z-20 w-2 cursor-col-resize transition-colors hover:bg-primary-500/30 active:bg-primary-500/40"
    use:resizeHandle={{
      axis: "x",
      direction: 1,
      min: 200,
      max: 480,
      storageKey: "layout.sidebarWidth",
      getSize: () => sidebarWidth,
      onResize: (size) => (sidebarWidth = size),
    }}
  ></div>
  </div>

  <section class="relative min-w-0 flex-1">
    <header class="flex h-14 items-center justify-between border-b border-surface-200-800 px-5 select-none" data-tauri-drag-region use:dragRegion>
      <div class="flex min-w-0 items-center gap-2">
        {#if thread?.parentThreadId}
          <TooltipButton label="Back to parent thread" onclick={openParentThread} aria-label="Back to parent thread" class="btn-icon btn-icon-sm shrink-0 hover:preset-tonal text-surface-500">
            <ArrowLeft size={15} />
          </TooltipButton>
        {/if}
        <ThreadDebugPopover {thread} codexHome={codexHome()}>
        <div class="min-w-0">
        <div class="truncate text-sm font-medium" title={thread?.title ?? project?.name ?? "Home"}>
          {#if thread?.parentThreadId}
            Subagent · {thread.agentNickname ?? thread.agentRole ?? thread.title}
          {:else}
            {thread?.title ?? project?.name ?? "Home"}
          {/if}
        </div>
        {#if project}<div class="truncate text-[10px] text-surface-500">{project.path}</div>{/if}
        </div>
        </ThreadDebugPopover>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        {#if (view.threadId || view.draftCwd) && !view.worktreesPath}
          <ThreadHeader
            codexHome={codexHome()}
            repoName={project?.name ?? null}
            repoDir={project?.path ?? view.draftCwd ?? thread?.cwd ?? ""}
            cwd={project?.workspaceId ? project.path : (thread?.cwd ?? view.draftCwd ?? project?.path ?? "")}
            threadId={view.threadId}
            onMovedToWorktree={movedToWorktree}
            onError={(message) => (handoff.error = message)}
          />
        {/if}
        <TooltipButton label="Refresh threads" onclick={refresh} aria-label="Refresh Codex threads" class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500">
          <RefreshCw size={15} class={appData.loading ? "animate-spin" : ""} />
        </TooltipButton>
      </div>
    </header>

    {#if handoff.error}
      <div class="pointer-events-none absolute inset-x-0 top-16 z-30 flex justify-center px-4">
        <div class="pointer-events-auto flex items-center gap-2 rounded-lg border border-surface-200-800 bg-surface-50-950 px-3 py-2 text-xs shadow-lg">
          <span class="text-error-500">{handoff.error}</span>
          <TooltipButton label="Dismiss" onclick={() => (handoff.error = null)} aria-label="Dismiss" class="btn-icon btn-icon-sm text-surface-500 hover:preset-tonal"><X size={13} /></TooltipButton>
        </div>
      </div>
    {/if}

    {#if review}
      <div class="h-[calc(100%-3.5rem)]">
        {#key review.path}
          <ReviewView repoDir={review.path} repoName={review.name} onBack={goHome} onAskCodex={askCodexReview} />
        {/key}
      </div>
    {:else if worktrees}
      <div class="h-[calc(100%-3.5rem)]">
        {#key worktrees.path}
          <Worktrees
            repoDir={worktrees.path}
            repoName={worktrees.name}
            projects={projects()}
            codexHome={codexHome()}
            onBack={goHome}
            onOpenInApp={focusProjectPath}
            onRevealInFinder={(path) => revealInFinder(path)}
            onNewThread={newThreadInDir}
            onReview={() => openReview(worktrees)}
            onRenameProject={renameProjectAt}
          />
        {/key}
      </div>
    {:else if (view.threadId || view.draftCwd) && !appData.error}
      <div class="h-[calc(100%-3.5rem)]">
        {#key view.epoch}
          <ThreadView
            threadId={view.threadId}
            cwd={view.draftCwd ?? project?.path ?? ""}
            projectPath={project?.path ?? view.draftCwd ?? ""}
            workspaceId={project?.workspaceId ?? null}
            codexHome={codexHome()}
            expectedCwd={expectedCwdFor(view.threadId)}
            sideQuestions={appData.data?.sideQuestions ?? []}
            onSelectThread={openThreadById}
            onOpenSubagent={openSubagent}
            onThreadCreated={threadCreated}
            onDataChanged={applyData}
            onCommand={slashCommand}
          />
        {/key}
      </div>
    {:else if detail && !appData.error}
      <div class="h-[calc(100%-3.5rem)]">
        {#key detail.path}
          <ProjectDetail project={detail} onOpenThread={openThreadById} onNewThread={newThread} onManageWorkspace={openWorkspaceDialog} />
        {/key}
      </div>
    {:else if appData.error}
      <div class="grid h-[calc(100%-3.5rem)] place-items-center p-8">
        <div class="card preset-tonal-error max-w-md p-4 text-sm">
          <div class="font-semibold">Could not connect to Codex</div>
          <p class="mt-1 text-xs leading-5">{appData.error}</p>
        </div>
      </div>
    {:else}
      <div class="h-[calc(100%-3.5rem)]">
        <HomePage
          projects={projects()}
          codexHome={codexHome()}
          codexBinary={codexBinary()}
          onAddProject={addProject}
          onAddWorkspace={() => openWorkspaceDialog()}
          onSelectThread={openThread}
          onNewThread={newThread}
          onMenuAction={menuAction}
          onSwitchHome={switchHome}
          onOpenWorktrees={openWorktrees}
          onOpenProject={openProjectDetail}
        />
      </div>
    {/if}
  </section>
</main>
{/if}

{#if settingsOpen}
  <div class="fixed inset-0 z-50">
    <SettingsView
      account={appData.data?.account ?? null}
      codexHome={appData.data?.codexHome ?? null}
      codexBinary={appData.data?.codexBinary ?? null}
      initialSection={settingsNav.section}
      focusServer={settingsNav.focusServer}
      focusTool={settingsNav.focusTool}
      navNonce={settingsNav.nonce}
      onClose={() => {
        settingsOpen = false;
        closeSettings();
      }}
    />
  </div>
{/if}

<DialogHost />
<ToastHost />
