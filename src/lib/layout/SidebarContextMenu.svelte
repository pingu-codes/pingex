<script lang="ts">
import {
  Archive,
  ArrowDown,
  ArrowDownToLine,
  ArrowUp,
  Bookmark,
  Eye,
  EyeOff,
  FolderGit2,
  FolderOpen,
  FolderPlus,
  GitBranch,
  GitFork,
  Layers3,
  ListRestart,
  Pencil,
  Pin,
  PinOff,
  Settings2,
  Star,
  StarOff,
  Trash2,
  X,
} from "@lucide/svelte";
import type { MenuAction, MenuTarget } from "$lib/types";

let {
  menu,
  sectionsSupported = false,
  onAct,
  onClose,
}: {
  menu: { x: number; y: number; target: MenuTarget };
  /** Whether this Codex has thread sections; hides the section items otherwise. */
  sectionsSupported?: boolean;
  onAct: (action: MenuAction) => void;
  onClose: () => void;
} = $props();

const menuIsPinned = (target: MenuTarget) =>
  target.kind === "project" ? target.project.pinned : target.kind === "thread" ? target.thread.pinned : false;
const hiddenCount = (target: MenuTarget) =>
  target.kind === "project" ? target.project.threads.filter((thread) => thread.hidden).length : 0;
</script>

<!-- Capture phase + preventDefault: the composer reads an unclaimed Escape as
     "interrupt the running turn". -->
<svelte:window
  onkeydowncapture={(event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    onClose();
  }}
/>

<div
  class="fixed inset-0 z-40"
  role="presentation"
  onclick={onClose}
  oncontextmenu={(event) => {
    event.preventDefault();
    onClose();
  }}
></div>
<div
  class="popover-card fixed z-50 w-[190px] select-none p-1"
  style="left: {menu.x}px; top: {menu.y}px"
  role="menu"
>
  {#if menu.target.kind === "folder"}
    <button
      role="menuitem"
      onclick={() => onAct("newFolder")}
      class="menu-row"
    >
      <FolderPlus size={13} class="text-surface-500" />
      New subfolder
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("rename")}
      class="menu-row"
    >
      <Pencil size={13} class="text-surface-500" />
      Rename folder
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("deleteFolder")}
      class="menu-row text-error-500"
      title="Its contents move up a level"
    >
      <Trash2 size={13} />
      Delete folder
    </button>
  {:else if menu.target.kind === "section"}
    <button
      role="menuitem"
      onclick={() => onAct("rename")}
      class="menu-row"
    >
      <Pencil size={13} class="text-surface-500" />
      Rename section
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("deleteSection")}
      class="menu-row text-error-500"
    >
      <Trash2 size={13} />
      Delete section
    </button>
  {:else}
  <button
    role="menuitem"
    onclick={() => onAct("reveal")}
    class="menu-row"
  >
    <FolderOpen size={13} class="text-surface-500" />
    Open in Finder
  </button>
  {#if menu.target.kind !== "project" || menu.target.project.kind !== "multiProject"}
    <button
      role="menuitem"
      onclick={() => onAct("rename")}
      class="menu-row"
    >
      <Pencil size={13} class="text-surface-500" />
      Rename {menu.target.kind}
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("togglePin")}
      class="menu-row"
    >
      {#if menu.target.kind === "thread"}
        {#if menuIsPinned(menu.target)}
          <StarOff size={13} class="text-surface-500" />
          Unfavorite thread
        {:else}
          <Star size={13} class="text-surface-500" />
          Favorite thread
        {/if}
      {:else if menuIsPinned(menu.target)}
        <PinOff size={13} class="text-surface-500" />
        Unpin {menu.target.kind}
      {:else}
        <Pin size={13} class="text-surface-500" />
        Pin {menu.target.kind}
      {/if}
    </button>
  {/if}
  {#if menu.target.kind === "thread"}
    <button
      role="menuitem"
      onclick={() => onAct("toggleHidden")}
      class="menu-row"
    >
      {#if menu.target.thread.hidden}
        <Eye size={13} class="text-surface-500" />
        Unhide thread
      {:else}
        <EyeOff size={13} class="text-surface-500" />
        Hide thread
      {/if}
    </button>
    {#if !menu.target.thread.hidden}
      <button
        role="menuitem"
        onclick={() => onAct("hideBelow")}
        class="menu-row"
        title="Hide every thread listed after this one"
      >
        <ArrowDownToLine size={13} class="text-surface-500" />
        Hide threads below
      </button>
    {/if}
  {/if}
  {#if menu.target.kind === "project"}
    <button
      role="menuitem"
      onclick={() => onAct("newFolder")}
      class="menu-row"
    >
      <FolderPlus size={13} class="text-surface-500" />
      New thread folder
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("openDetails")}
      class="menu-row"
    >
      <Settings2 size={13} class="text-surface-500" />
      {menu.target.project.kind === "multiProject" ? "Workspace details" : "Project details"}
    </button>
    {#if menu.target.project.kind !== "multiProject"}
      <button
        role="menuitem"
        onclick={() => onAct("openGit")}
        class="menu-row"
      >
        <GitBranch size={13} class="text-surface-500" />
        Git
      </button>
      <button
        role="menuitem"
        onclick={() => onAct("openWorktrees")}
        class="menu-row"
      >
        <FolderGit2 size={13} class="text-surface-500" />
        Worktrees
      </button>
    {/if}
    {#if menu.target.project.kind === "folder"}
      <button
        role="menuitem"
        onclick={() => onAct("addWorktree")}
        class="menu-row"
        title="Add an existing worktree of this repository to the sidebar"
      >
        <FolderGit2 size={13} class="text-surface-500" />
        Add worktree
      </button>
    {/if}
    {#if menu.target.project.kind !== "multiProject"}
    <button
      role="menuitem"
      onclick={() => onAct("resetOrder")}
      class="menu-row"
      title="Forget drag ordering: favorites first, then most recent"
    >
      <ListRestart size={13} class="text-surface-500" />
      Reset thread order
    </button>
    {#if hiddenCount(menu.target) > 0}
      <button
        role="menuitem"
        onclick={() => onAct("unhideAll")}
        class="menu-row"
      >
        <Eye size={13} class="text-surface-500" />
        Unhide {hiddenCount(menu.target)} thread{hiddenCount(menu.target) === 1 ? "" : "s"}
      </button>
    {/if}
    <button
      role="menuitem"
      onclick={() => onAct("toggleArchive")}
      class="menu-row"
    >
      <Archive size={13} class="text-surface-500" />
      {menu.target.project.kind === "worktree" ? "Hide worktree" : "Archive project"}
    </button>
    {/if}
  {/if}
  {#if menu.target.kind === "project" && menu.target.project.kind !== "worktree" && menu.target.project.kind !== "multiProject"}
    <button
      role="menuitem"
      onclick={() => onAct("moveUp")}
      class="menu-row"
    >
      <ArrowUp size={13} class="text-surface-500" />
      Move up
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("moveDown")}
      class="menu-row"
    >
      <ArrowDown size={13} class="text-surface-500" />
      Move down
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("remove")}
      class="menu-row text-error-500"
    >
      <X size={13} />
      Remove project
    </button>
  {/if}
  {#if menu.target.kind === "thread"}
    <button
      role="menuitem"
      onclick={() => onAct("moveToWorkspace")}
      class="menu-row"
    >
      <Layers3 size={13} class="text-surface-500" />
      Move to workspace
    </button>
    {#if sectionsSupported}
      <button
        role="menuitem"
        onclick={() => onAct("moveToSection")}
        class="menu-row"
      >
        <Bookmark size={13} class="text-surface-500" />
        Move to section
      </button>
    {/if}
    {#if menu.target.project.kind !== "multiProject"}
      <button
        role="menuitem"
        onclick={() => onAct("createWorktree")}
        class="menu-row"
      >
        <GitBranch size={13} class="text-surface-500" />
        Create worktree
      </button>
    {/if}
    <button
      role="menuitem"
      onclick={() => onAct("fork")}
      class="menu-row"
    >
      <GitFork size={13} class="text-surface-500" />
      Fork thread
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("archive")}
      class="menu-row"
    >
      <Archive size={13} class="text-surface-500" />
      Archive thread
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("delete")}
      class="menu-row text-error-500"
    >
      <Trash2 size={13} />
      Delete thread
    </button>
  {/if}
  {/if}
</div>
