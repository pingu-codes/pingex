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
  import { Menu, Portal, type useMenu } from "@skeletonlabs/skeleton-svelte";
  import type { Component } from "svelte";
  import type { MenuAction, MenuTarget } from "$lib/types";
  import { can } from "$lib/utils/capabilities";

  let {
    menu,
    target,
    sectionsSupported = false,
  }: {
    /** The sidebar's row menu; its triggers live on the rows themselves. */
    menu: ReturnType<typeof useMenu>;
    /** The row the menu was last opened for. */
    target: MenuTarget | null;
    /** Whether this Codex has thread sections; hides the section items otherwise. */
    sectionsSupported?: boolean;
  } = $props();

  type Entry = {
    action: MenuAction;
    icon: Component<{ size?: number; class?: string }>;
    label: string;
    danger?: boolean;
    title?: string;
    show?: boolean;
  };

  function entries(target: MenuTarget): Entry[] {
    if (target.kind === "folder") {
      return [
        { action: "newFolder", icon: FolderPlus, label: "New subfolder" },
        { action: "rename", icon: Pencil, label: "Rename folder" },
        {
          action: "deleteFolder",
          icon: Trash2,
          label: "Delete folder",
          danger: true,
          title: "Its contents move up a level",
        },
      ];
    }
    if (target.kind === "section") {
      return [
        { action: "rename", icon: Pencil, label: "Rename section" },
        { action: "deleteSection", icon: Trash2, label: "Delete section", danger: true },
      ];
    }
    const project = target.project;
    const multi = project.kind === "multiProject";
    const pinned = target.kind === "project" ? project.pinned : target.thread.pinned;
    const pin: Pick<Entry, "icon" | "label"> =
      target.kind === "thread"
        ? pinned
          ? { icon: StarOff, label: "Unfavorite thread" }
          : { icon: Star, label: "Favorite thread" }
        : pinned
          ? { icon: PinOff, label: `Unpin ${target.kind}` }
          : { icon: Pin, label: `Pin ${target.kind}` };
    // A workspace row can't be renamed or pinned; threads inside one can.
    const workspaceRow = target.kind === "project" && multi;
    const shared: Entry[] = [
      { action: "reveal", icon: FolderOpen, label: "Open in Finder" },
      { action: "rename", icon: Pencil, label: `Rename ${target.kind}`, show: !workspaceRow },
      { action: "togglePin", ...pin, show: !workspaceRow },
    ];
    if (target.kind === "thread") {
      const hidden = target.thread.hidden;
      return [
        ...shared,
        hidden
          ? { action: "toggleHidden", icon: Eye, label: "Unhide thread" }
          : { action: "toggleHidden", icon: EyeOff, label: "Hide thread" },
        {
          action: "hideBelow",
          icon: ArrowDownToLine,
          label: "Hide threads below",
          title: "Hide every thread listed after this one",
          show: !hidden,
        },
        { action: "moveToWorkspace", icon: Layers3, label: "Move to workspace" },
        { action: "moveToSection", icon: Bookmark, label: "Move to section", show: sectionsSupported },
        { action: "createWorktree", icon: GitBranch, label: "Create worktree", show: !multi },
        { action: "fork", icon: GitFork, label: "Fork thread", show: can(target.thread.harness, "fork") },
        { action: "archive", icon: Archive, label: "Archive thread" },
        { action: "delete", icon: Trash2, label: "Delete thread", danger: true },
      ];
    }
    const hiddenCount = project.threads.filter((thread) => thread.hidden).length;
    const reorderable = project.kind !== "worktree" && !multi;
    return [
      ...shared,
      { action: "newFolder", icon: FolderPlus, label: "New thread folder" },
      { action: "openDetails", icon: Settings2, label: multi ? "Workspace details" : "Project details" },
      { action: "openGit", icon: GitBranch, label: "Git", show: !multi },
      { action: "openWorktrees", icon: FolderGit2, label: "Worktrees", show: !multi },
      {
        action: "addWorktree",
        icon: FolderGit2,
        label: "Add worktree",
        title: "Add an existing worktree of this repository to the sidebar",
        show: project.kind === "folder",
      },
      {
        action: "resetOrder",
        icon: ListRestart,
        label: "Reset thread order",
        title: "Forget drag ordering: favorites first, then most recent",
        show: !multi,
      },
      {
        action: "unhideAll",
        icon: Eye,
        label: `Unhide ${hiddenCount} thread${hiddenCount === 1 ? "" : "s"}`,
        show: !multi && hiddenCount > 0,
      },
      {
        action: "toggleArchive",
        icon: Archive,
        label: project.kind === "worktree" ? "Hide worktree" : "Archive project",
        show: !multi,
      },
      { action: "moveUp", icon: ArrowUp, label: "Move up", show: reorderable },
      { action: "moveDown", icon: ArrowDown, label: "Move down", show: reorderable },
      { action: "remove", icon: X, label: "Remove project", danger: true, show: reorderable },
    ];
  }

  const visible = $derived(target ? entries(target).filter((entry) => entry.show !== false) : []);
</script>

<Menu.Provider value={menu}>
  <Portal>
    <Menu.Positioner>
      <Menu.Content class="popover-card z-50 w-[190px] min-w-0 gap-0 select-none p-1 outline-none">
        {#if menu().open}
          {#each visible as entry (entry.action)}
            <Menu.Item
              value={entry.action}
              title={entry.title}
              class="menu-row cursor-pointer justify-start {entry.danger ? "text-error-500" : ""}"
            >
              <entry.icon size={13} class={entry.danger ? "" : "text-surface-500"} />
              <Menu.ItemText>{entry.label}</Menu.ItemText>
            </Menu.Item>
          {/each}
        {/if}
      </Menu.Content>
    </Menu.Positioner>
  </Portal>
</Menu.Provider>
