<script lang="ts">
import {
  Archive,
  ArrowDown,
  ArrowUp,
  Bookmark,
  FolderOpen,
  FolderPlus,
  GitFork,
  Layers3,
  Pencil,
  Pin,
  PinOff,
  Settings2,
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
</script>

<svelte:window onkeydown={(event) => event.key === "Escape" && onClose()} />

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
  class="card fixed z-50 w-[190px] select-none border border-surface-200-800 bg-surface-50-950 p-1 shadow-xl"
  style="left: {menu.x}px; top: {menu.y}px"
  role="menu"
>
  {#if menu.target.kind === "folder"}
    <button
      role="menuitem"
      onclick={() => onAct("newFolder")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <FolderPlus size={13} class="text-surface-500" />
      New subfolder
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("rename")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <Pencil size={13} class="text-surface-500" />
      Rename folder
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("deleteFolder")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] text-error-500 hover:preset-tonal"
      title="Its contents move up a level"
    >
      <Trash2 size={13} />
      Delete folder
    </button>
  {:else if menu.target.kind === "section"}
    <button
      role="menuitem"
      onclick={() => onAct("rename")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <Pencil size={13} class="text-surface-500" />
      Rename section
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("deleteSection")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] text-error-500 hover:preset-tonal"
    >
      <Trash2 size={13} />
      Delete section
    </button>
  {:else}
  <button
    role="menuitem"
    onclick={() => onAct("reveal")}
    class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
  >
    <FolderOpen size={13} class="text-surface-500" />
    Open in Finder
  </button>
  {#if menu.target.kind !== "project" || menu.target.project.kind !== "multiProject"}
    <button
      role="menuitem"
      onclick={() => onAct("rename")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <Pencil size={13} class="text-surface-500" />
      Rename {menu.target.kind}
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("togglePin")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      {#if menuIsPinned(menu.target)}
        <PinOff size={13} class="text-surface-500" />
        Unpin {menu.target.kind}
      {:else}
        <Pin size={13} class="text-surface-500" />
        Pin {menu.target.kind}
      {/if}
    </button>
  {/if}
  {#if menu.target.kind === "project"}
    <button
      role="menuitem"
      onclick={() => onAct("newFolder")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <FolderPlus size={13} class="text-surface-500" />
      New thread folder
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("openDetails")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <Settings2 size={13} class="text-surface-500" />
      {menu.target.project.kind === "multiProject" ? "Workspace details" : "Project details"}
    </button>
    {#if menu.target.project.kind !== "multiProject"}
    <button
      role="menuitem"
      onclick={() => onAct("toggleArchive")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
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
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <ArrowUp size={13} class="text-surface-500" />
      Move up
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("moveDown")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <ArrowDown size={13} class="text-surface-500" />
      Move down
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("remove")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] text-error-500 hover:preset-tonal"
    >
      <X size={13} />
      Remove project
    </button>
  {/if}
  {#if menu.target.kind === "thread"}
    <button
      role="menuitem"
      onclick={() => onAct("moveToWorkspace")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <Layers3 size={13} class="text-surface-500" />
      Move to workspace
    </button>
    {#if sectionsSupported}
      <button
        role="menuitem"
        onclick={() => onAct("moveToSection")}
        class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
      >
        <Bookmark size={13} class="text-surface-500" />
        Move to section
      </button>
    {/if}
    <button
      role="menuitem"
      onclick={() => onAct("fork")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <GitFork size={13} class="text-surface-500" />
      Fork thread
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("archive")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] hover:preset-tonal"
    >
      <Archive size={13} class="text-surface-500" />
      Archive thread
    </button>
    <button
      role="menuitem"
      onclick={() => onAct("delete")}
      class="flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[12px] text-error-500 hover:preset-tonal"
    >
      <Trash2 size={13} />
      Delete thread
    </button>
  {/if}
  {/if}
</div>
