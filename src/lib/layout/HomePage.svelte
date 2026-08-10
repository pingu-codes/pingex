<script lang="ts">
import {
  Archive,
  ArchiveRestore,
  Boxes,
  ChevronDown,
  Cpu,
  EyeOff,
  Folder,
  FolderGit2,
  FolderOpen,
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
import { Portal, Tooltip } from "@skeletonlabs/skeleton-svelte";
import { readHomeOverview } from "$lib/services/api";
import type { HomeOverview, MenuTarget, Project, ThreadSummary } from "$lib/types";
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

let hiddenOpen = $state(false);
let overview = $state<HomeOverview | null>(null);

// The active home's defaults (model, MCP servers, skills). Read-only and
// best-effort — failures leave the dashboard's config panels empty.
$effect(() => {
  let cancelled = false;
  readHomeOverview()
    .then((result) => {
      if (!cancelled) overview = result;
    })
    .catch(() => {
      if (!cancelled) overview = null;
    });
  return () => {
    cancelled = true;
  };
});

const target = (project: Project): MenuTarget => ({ kind: "project", project });

const relativeTime = (timestamp: number) => {
  const days = Math.floor((Date.now() / 1000 - timestamp) / 86400);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  return `${days}d ago`;
};
</script>

{#snippet iconAction(label: string, onclick: () => void, danger = false)}
  <Tooltip openDelay={350}>
    <Tooltip.Trigger
      {onclick}
      aria-label={label}
      class="btn-icon btn-icon-sm hover:preset-tonal {danger ? 'text-surface-500 hover:text-error-500' : 'text-surface-500'} opacity-0 transition focus:opacity-100 group-hover/row:opacity-100"
    >
      {#if label.startsWith("Unpin")}<PinOff size={14} />
      {:else if label.startsWith("Pin")}<Pin size={14} />
      {:else if label.startsWith("Rename")}<Pencil size={14} />
      {:else if label.startsWith("Reveal")}<FolderOpen size={14} />
      {:else if label.startsWith("Hide")}<EyeOff size={14} />
      {:else if label.startsWith("Archive")}<Archive size={14} />
      {:else if label.startsWith("Show") || label.startsWith("Restore")}<ArchiveRestore size={14} />
      {:else if label.startsWith("New thread")}<SquarePen size={14} />
      {:else if label.startsWith("Project details")}<Settings2 size={14} />
      {:else}<X size={14} />{/if}
    </Tooltip.Trigger>
    <Portal>
      <Tooltip.Positioner>
        <Tooltip.Content class="card preset-filled z-50 px-2 py-1 text-xs shadow-lg">{label}</Tooltip.Content>
      </Tooltip.Positioner>
    </Portal>
  </Tooltip>
{/snippet}

{#snippet projectRow(project: Project, hideLabel: string)}
  <div class="group/row flex items-center gap-3 rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2.5">
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
      {@render iconAction("New thread", () => onNewThread(project))}
      {#if onOpenProject}
        {@render iconAction(project.kind === "multiProject" ? "Workspace details" : "Project details", () => onOpenProject?.(project))}
      {/if}
      {#if project.kind !== "multiProject"}
        {@render iconAction(project.pinned ? "Unpin project" : "Pin project", () => onMenuAction("togglePin", target(project)))}
        {@render iconAction("Rename project", () => onMenuAction("rename", target(project)))}
      {/if}
      {@render iconAction("Reveal in Finder", () => onMenuAction("reveal", target(project)))}
      {#if project.kind !== "multiProject"}
        {@render iconAction(hideLabel, () => onMenuAction("toggleArchive", target(project)))}
      {/if}
      {#if project.kind === "folder"}
        {@render iconAction("Remove project", () => onMenuAction("remove", target(project)), true)}
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

    <div class="mt-6 rounded-xl border border-surface-200-800 bg-surface-100-900 p-4">
      <div class="flex items-start gap-3">
        <div class="grid size-9 shrink-0 place-items-center rounded-lg preset-tonal-primary">
          <House size={18} strokeWidth={1.8} />
        </div>
        <div class="min-w-0 flex-1">
          <div class="text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Codex home</div>
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
        {#each [["Projects", folders.length], ["Worktrees", worktrees.length], ["Threads", threadCount], ["Pinned", pinnedProjectCount]] as [label, value] (label)}
          <div class="rounded-lg bg-surface-50-950 px-3 py-2">
            <div class="text-lg font-semibold tabular-nums">{value}</div>
            <div class="text-[11px] text-surface-500">{label}</div>
          </div>
        {/each}
      </div>
    </div>

    <div class="mt-4 grid gap-3 sm:grid-cols-2">
      <div class="rounded-xl border border-surface-200-800 bg-surface-100-900 p-4">
        <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">
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

      <div class="rounded-xl border border-surface-200-800 bg-surface-100-900 p-4">
        <div class="flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">
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
      <h2 class="mt-8 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Pinned threads</h2>
      <div class="mt-2 space-y-1.5">
        {#each pinnedThreads as entry (entry.thread.id)}
          <button
            onclick={() => onSelectThread(entry.project, entry.thread.id)}
            class="flex w-full items-center gap-2.5 rounded-xl border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-left hover:preset-tonal"
          >
            <Star size={13} class="shrink-0 fill-warning-500 text-warning-500" />
            <span class="min-w-0 flex-1 truncate text-sm">{entry.thread.title}</span>
            <span class="shrink-0 text-[11px] text-surface-500">{entry.project.name}</span>
            <span class="shrink-0 text-[11px] text-surface-500">{relativeTime(entry.thread.updatedAt)}</span>
          </button>
        {/each}
      </div>
    {/if}

    <h2 class="mt-8 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Projects</h2>
    {#if folders.length === 0}
      <button
        onclick={onAddProject}
        class="mt-2 w-full rounded-xl border border-dashed border-surface-300-700 px-4 py-6 text-left text-xs leading-5 text-surface-600-400 hover:border-surface-400-600 hover:text-surface-800-200"
      >
        Add a folder to see its Codex threads here.
      </button>
    {:else}
      <div class="mt-2 space-y-1.5">
        {#each folders as project (project.path)}
          {@render projectRow(project, "Archive project")}
        {/each}
      </div>
    {/if}

    {#if worktrees.length > 0}
      <h2 class="mt-8 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Imported from Codex</h2>
      <p class="mt-1 text-xs text-surface-500">
        Permanent worktrees discovered in your Codex home. Hide the ones you don't need.
      </p>
      <div class="mt-2 space-y-1.5">
        {#each worktrees as project (project.path)}
          {@render projectRow(project, "Hide worktree")}
        {/each}
      </div>
    {/if}

    {#if workspaces.length > 0}
      <h2 class="mt-8 text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500">Workspaces</h2>
      <p class="mt-1 text-xs text-surface-500">Writable shared hubs with isolated project members.</p>
      <div class="mt-2 space-y-1.5">
        {#each workspaces as project (project.path)}
          {@render projectRow(project, "Archive workspace")}
        {/each}
      </div>
    {/if}

    {#if hidden.length > 0}
      <button
        onclick={() => (hiddenOpen = !hiddenOpen)}
        class="mt-8 flex w-full items-center gap-2 text-left text-[11px] font-semibold uppercase tracking-[0.08em] text-surface-500 hover:text-surface-700-300"
      >
        <span>Archived &amp; hidden</span>
        <span class="rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] font-medium normal-case tracking-normal">{hidden.length}</span>
        <ChevronDown size={12} class="transition {hiddenOpen ? '' : '-rotate-90'}" />
      </button>
      {#if hiddenOpen}
        <div class="mt-2 space-y-1.5">
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
        </div>
      {/if}
    {/if}
  </div>
</div>
