<script lang="ts">
import {
  AppWindow,
  Archive,
  ArchiveRestore,
  Boxes,
  ChevronDown,
  Cpu,
  EyeOff,
  Folder,
  FolderGit2,
  FolderOpen,
  Gauge,
  House,
  Layers3,
  Pencil,
  Pin,
  PinOff,
  Plus,
  Server,
  Settings2,
  Sparkles,
  SquarePen,
  Star,
  X,
} from "@lucide/svelte";
import { Collapsible } from "@skeletonlabs/skeleton-svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { commands } from "$lib/services/homeRouting";
import { openSettings } from "$lib/services/settingsNav.svelte";
import { isTauri } from "$lib/services/tauri";
import type { HomeOverview, MenuTarget, Project, ThreadSummary, UsageBreakdown } from "$lib/types";
import { costFor, costLabel, rangeSince } from "$lib/usage/usageBreakdown";
import { formatTokensShort, relativeTime } from "$lib/utils/format";
import { resource } from "$lib/utils/resource.svelte";
import BranchChip from "$lib/worktrees/BranchChip.svelte";

type HomeAction = "reveal" | "rename" | "togglePin" | "toggleArchive" | "remove";

let {
  projects,
  codexHome = null,
  codexBinary = null,
  onAddProject,
  onAddWorkspace,
  onSelectThread,
  onNewThread,
  onMenuAction,
  onSwitchHome,
  onOpenWorktrees,
  onOpenProject,
}: {
  projects: Project[];
  codexHome?: string | null;
  codexBinary?: string | null;
  onAddProject: () => void;
  onAddWorkspace?: () => void;
  onSelectThread: (project: Project, threadId: string) => void;
  onNewThread: (project: Project) => void;
  onMenuAction: (action: HomeAction, target: MenuTarget) => void;
  onSwitchHome?: () => void;
  onOpenWorktrees?: (project: Project) => void;
  onOpenProject?: (project: Project) => void;
} = $props();

const active = $derived(projects.filter((project) => !project.archived));
const folders = $derived(active.filter((project) => project.kind === "folder"));
const worktrees = $derived(active.filter((project) => project.kind === "worktree"));
const workspaces = $derived(active.filter((project) => project.kind === "multiProject"));
const hidden = $derived(projects.filter((project) => project.archived));
const pinnedThreads = $derived(
  active.flatMap((project) => project.threads.filter((thread) => thread.pinned).map((thread) => ({ project, thread }))),
);
const threadCount = $derived(active.reduce((total, project) => total + project.threads.length, 0));
const pinnedProjectCount = $derived(active.filter((project) => project.pinned).length);

// Last 30 days of spend across the Home, and the Home's defaults (model, MCP
// servers, skills). Both are read-only and best-effort: a failure leaves its
// panel empty.
const usageLoad = resource<UsageBreakdown | null>(
  () => commands.readUsageBreakdown({ kind: "global" }, rangeSince("30d")),
  { initial: null },
);
const overviewLoad = resource<HomeOverview | null>(() => commands.readHomeOverview(), { initial: null });
$effect(() => {
  void usageLoad.reload();
  void overviewLoad.reload();
});
const usage = $derived(usageLoad.value);
const overview = $derived(overviewLoad.value);
const usageCostSummary = $derived(usage ? costFor(usage.byModel, usage.reportedCostUsd) : null);
const usageTiles = $derived(
  usage
    ? [
        ["Tokens", formatTokensShort(usage.tokens.totalTokens)],
        ["Output", formatTokensShort(usage.tokens.outputTokens)],
        ["Turns", usage.turns],
        [usageCostSummary?.estimated ? "Est. cost" : "Cost", costLabel(usageCostSummary) ?? "—"],
      ]
    : [],
);

const projectSections = $derived([
  { title: "Projects", list: folders, hideLabel: "Archive project", hideIcon: Archive, emptyCta: true },
  {
    title: "Imported from Codex",
    blurb: "Permanent worktrees discovered in your Codex home. Hide the ones you don't need.",
    list: worktrees,
    hideLabel: "Hide worktree",
    hideIcon: EyeOff,
  },
  {
    title: "Workspaces",
    blurb: "Writable shared hubs with isolated project members.",
    list: workspaces,
    hideLabel: "Archive workspace",
    hideIcon: Archive,
  },
]);

const target = (project: Project): MenuTarget => ({ kind: "project", project });
</script>

{#snippet iconAction(label: string, Icon: typeof X, onclick: () => void, danger = false)}
  <TooltipButton
    {label}
    {onclick}
    class="btn-icon btn-icon-sm hover:preset-tonal {danger ? 'text-surface-500 hover:text-error-500' : 'text-surface-500'} opacity-0 transition focus:opacity-100 group-hover/row:opacity-100"
  >
    <Icon size={14} />
  </TooltipButton>
{/snippet}

{#snippet statTiles(tiles: (string | number)[][])}
  {#each tiles as [label, value] (label)}
    <div class="rounded-lg bg-surface-50-950 px-3 py-2">
      <div class="text-lg font-semibold tabular-nums">{value}</div>
      <div class="text-[11px] text-surface-500">{label}</div>
    </div>
  {/each}
{/snippet}

{#snippet projectRow(project: Project, hideLabel: string, HideIcon: typeof X)}
  <div class="group/row flex items-center gap-3 panel-card px-3 py-2.5">
    {#if project.kind === "multiProject"}
      <Layers3 size={17} strokeWidth={1.7} class="shrink-0 text-primary-500" />
    {:else if project.kind === "worktree"}
      <FolderGit2 size={17} strokeWidth={1.7} class="shrink-0 text-primary-500" />
    {:else}
      <Folder size={17} strokeWidth={1.7} class="shrink-0 text-surface-500" />
    {/if}
    <button onclick={() => onNewThread(project)} class="min-w-0 flex-1 text-left" title="Start a new thread in {project.name}">
      <div class="flex items-center gap-1.5">
        <span class="truncate text-sm font-medium">{project.name}</span>
        {#if project.pinned}<Pin size={11} class="shrink-0 text-surface-500" />{/if}
      </div>
      <div class="truncate text-[11px] text-surface-500">{project.path}</div>
    </button>
    <div class="flex shrink-0 items-center gap-2 group-hover/row:hidden">
      {#if onOpenWorktrees}
        <BranchChip path={project.path} onOpen={() => onOpenWorktrees?.(project)} />
      {/if}
      <span class="text-[11px] text-surface-500">
        {project.threads.length} thread{project.threads.length === 1 ? "" : "s"}
      </span>
    </div>
    <div class="hidden shrink-0 items-center group-hover/row:flex">
      {@render iconAction("New thread", SquarePen, () => onNewThread(project))}
      {#if onOpenProject}
        {@render iconAction(project.kind === "multiProject" ? "Workspace details" : "Project details", Settings2, () => onOpenProject?.(project))}
      {/if}
      {#if project.kind !== "multiProject"}
        {@render iconAction(project.pinned ? "Unpin project" : "Pin project", project.pinned ? PinOff : Pin, () => onMenuAction("togglePin", target(project)))}
        {@render iconAction("Rename project", Pencil, () => onMenuAction("rename", target(project)))}
      {/if}
      {@render iconAction("Reveal in Finder", FolderOpen, () => onMenuAction("reveal", target(project)))}
      {#if project.kind !== "multiProject"}
        {@render iconAction(hideLabel, HideIcon, () => onMenuAction("toggleArchive", target(project)))}
      {/if}
      {#if project.kind === "folder"}
        {@render iconAction("Remove project", X, () => onMenuAction("remove", target(project)), true)}
      {/if}
    </div>
  </div>
{/snippet}

<div class="h-full overflow-y-auto">
  <div class="mx-auto max-w-3xl px-6 py-8">
    <div class="flex items-center justify-between">
      <div>
        <h1 class="text-lg font-semibold tracking-[-0.02em]">Home</h1>
        <p class="mt-0.5 text-xs text-surface-500">Manage your projects and Codex imports.</p>
      </div>
      <div class="flex gap-2">
        {#if onAddWorkspace}<button onclick={onAddWorkspace} class="btn btn-sm preset-tonal"><Layers3 size={14} />Workspace</button>{/if}
        <button onclick={onAddProject} class="btn btn-sm preset-filled-primary-500"><Plus size={14} />Add project</button>
      </div>
    </div>

    <div class="mt-6 panel-card p-4">
      <div class="flex items-start gap-3">
        <div class="grid size-9 shrink-0 place-items-center rounded-lg preset-tonal-primary">
          <House size={18} strokeWidth={1.8} />
        </div>
        <div class="min-w-0 flex-1">
          <div class="eyebrow">Codex home</div>
          <div class="mt-0.5 truncate font-mono text-sm" title={overview?.codexHome ?? codexHome ?? undefined}>
            {overview?.codexHome ?? codexHome ?? "~/.codex"}
          </div>
          <div class="mt-0.5 truncate font-mono text-[11px] text-surface-500">
            binary: {overview?.codexBinary ?? codexBinary ?? "codex"}
          </div>
        </div>
        {#if onSwitchHome}
          <button onclick={onSwitchHome} class="btn btn-sm preset-tonal shrink-0">
            <FolderOpen size={14} />
            Switch home
          </button>
        {/if}
        {#if isTauri()}
          <!-- A second account side by side: a fresh window with its own picker. -->
          <button onclick={() => void commands.openHomeWindow(null, null)} class="btn btn-sm preset-tonal shrink-0" title="Open another window, e.g. for a second account">
            <AppWindow size={14} />
            New window
          </button>
        {/if}
      </div>

      {#if overview && (overview.model || overview.reasoningEffort || overview.approvalPolicy || overview.sandboxMode)}
        <div class="mt-3 flex flex-wrap gap-1.5">
          {#if overview.model}
            <span class="inline-flex items-center gap-1 rounded-full bg-surface-200-800 px-2 py-0.5 text-[11px]">
              <Cpu size={11} class="text-surface-500" />{overview.model}
            </span>
          {/if}
          {#if overview.reasoningEffort}
            <span class="rounded-full bg-surface-200-800 px-2 py-0.5 text-[11px]">effort: {overview.reasoningEffort}</span>
          {/if}
          {#if overview.approvalPolicy}
            <span class="rounded-full bg-surface-200-800 px-2 py-0.5 text-[11px]">approval: {overview.approvalPolicy}</span>
          {/if}
          {#if overview.sandboxMode}
            <span class="rounded-full bg-surface-200-800 px-2 py-0.5 text-[11px]">sandbox: {overview.sandboxMode}</span>
          {/if}
        </div>
      {/if}

      <div class="mt-4 grid grid-cols-2 gap-2 sm:grid-cols-4">
        {@render statTiles([["Projects", folders.length], ["Worktrees", worktrees.length], ["Threads", threadCount], ["Pinned", pinnedProjectCount]])}
      </div>
    </div>

    <div class="mt-4 panel-card p-4" data-testid="home-usage-card">
      <div class="flex items-center gap-1.5 eyebrow">
        <Gauge size={13} />
        Usage · last 30 days
        <button type="button" class="ml-auto btn btn-sm preset-tonal normal-case tracking-normal" onclick={() => openSettings("usage")}>
          Details
        </button>
      </div>
      {#if usage && usage.turns > 0}
        <div class="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
          {@render statTiles(usageTiles)}
        </div>
      {:else}
        <p class="mt-2 text-xs text-surface-500">No turns recorded in the last 30 days.</p>
      {/if}
    </div>

    <div class="mt-4 grid gap-3 sm:grid-cols-2">
      <div class="panel-card p-4">
        <div class="flex items-center gap-1.5 eyebrow">
          <Sparkles size={13} />
          Skills
          <span class="rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal">
            {overview?.skills.length ?? 0}
          </span>
        </div>
        {#if overview && overview.skills.length > 0}
          <div class="mt-2 flex flex-wrap gap-1.5">
            {#each overview.skills as skill (skill.name)}
              <span class="rounded-md bg-surface-200-800 px-2 py-0.5 font-mono text-[11px]">{skill.name}</span>
            {/each}
          </div>
        {:else}
          <p class="mt-2 text-xs text-surface-500">No skills found in this home.</p>
        {/if}
      </div>

      <div class="panel-card p-4">
        <div class="flex items-center gap-1.5 eyebrow">
          <Server size={13} />
          MCP servers
          <span class="rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal">
            {overview?.mcpServers.length ?? 0}
          </span>
        </div>
        {#if overview && overview.mcpServers.length > 0}
          <div class="mt-2 space-y-1.5">
            {#each overview.mcpServers as server (server.name)}
              <div class="flex items-center gap-2">
                <Boxes size={13} class="shrink-0 text-surface-500" />
                <span class="shrink-0 text-xs font-medium">{server.name}</span>
                {#if server.command}
                  <span class="truncate font-mono text-[11px] text-surface-500" title={server.command}>{server.command}</span>
                {/if}
              </div>
            {/each}
          </div>
        {:else}
          <p class="mt-2 text-xs text-surface-500">No MCP servers configured.</p>
        {/if}
      </div>
    </div>

    {#if pinnedThreads.length > 0}
      <h2 class="mt-8 eyebrow">Pinned threads</h2>
      <div class="mt-2 space-y-1.5">
        {#each pinnedThreads as entry (entry.thread.id)}
          <button
            onclick={() => onSelectThread(entry.project, entry.thread.id)}
            class="flex w-full items-center gap-2.5 panel-card px-3 py-2 text-left hover:preset-tonal"
          >
            <Star size={13} class="shrink-0 fill-warning-500 text-warning-500" />
            <span class="min-w-0 flex-1 truncate text-sm">{entry.thread.title}</span>
            <span class="shrink-0 text-[11px] text-surface-500">{entry.project.name}</span>
            <span class="shrink-0 text-[11px] text-surface-500">{relativeTime(entry.thread.updatedAt, { suffix: " ago" })}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#each projectSections as section (section.title)}
      {#if section.list.length > 0 || section.emptyCta}
        <h2 class="mt-8 eyebrow">{section.title}</h2>
        {#if section.blurb}
          <p class="mt-1 text-xs text-surface-500">{section.blurb}</p>
        {/if}
        {#if section.list.length === 0}
          <button onclick={onAddProject} class="mt-2 w-full empty-cta">Add a folder to see its Codex threads here.</button>
        {:else}
          <div class="mt-2 space-y-1.5">
            {#each section.list as project (project.path)}
              {@render projectRow(project, section.hideLabel, section.hideIcon)}
            {/each}
          </div>
        {/if}
      {/if}
    {/each}

    {#if hidden.length > 0}
      <Collapsible class="mt-8 items-stretch gap-0">
        <Collapsible.Trigger
          class="group flex w-full items-center gap-2 text-left eyebrow hover:text-surface-700-300"
        >
          <span>Archived &amp; hidden</span>
          <span class="rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal">{hidden.length}</span>
          <ChevronDown size={12} class="transition group-data-[state=closed]:-rotate-90" />
        </Collapsible.Trigger>
        <Collapsible.Content class="mt-2 space-y-1.5">
          {#each hidden as project (project.path)}
            <div class="group/row flex items-center gap-3 rounded-xl border border-surface-200-800 px-3 py-2.5 opacity-70 transition hover:opacity-100">
              {#if project.kind === "worktree"}
                <FolderGit2 size={17} strokeWidth={1.7} class="shrink-0 text-surface-500" />
              {:else}
                <Folder size={17} strokeWidth={1.7} class="shrink-0 text-surface-500" />
              {/if}
              <div class="min-w-0 flex-1">
                <div class="truncate text-sm font-medium">{project.name}</div>
                <div class="truncate text-[11px] text-surface-500">{project.path}</div>
              </div>
              <button
                onclick={() => onMenuAction("toggleArchive", target(project))}
                class="btn btn-sm preset-tonal shrink-0"
              >
                <ArchiveRestore size={13} />
                {project.kind === "worktree" ? "Show" : "Restore"}
              </button>
            </div>
          {/each}
        </Collapsible.Content>
      </Collapsible>
    {/if}
  </div>
</div>
