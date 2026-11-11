<!--
  The review page's file rail: the changed files as a directory tree, each file
  with its counts and viewed mark, the current one highlighted and kept in
  view. Directories fold; the rail resizes from its right edge and folds to a
  strip, both remembered across PRs.
-->
<script lang="ts">
  import { Check, ChevronRight, ChevronsDownUp, ChevronsUpDown, PanelLeftClose, PanelLeftOpen } from "@lucide/svelte";
  import { createTreeViewCollection, TreeView } from "@skeletonlabs/skeleton-svelte";
  import { untrack } from "svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { ancestorsOf, buildFileTree, directoryIds, type FileTreeNode } from "$lib/review/fileTree";
  import type { FileCounts } from "$lib/review/reviewPage";
  import { loadFileRailCollapsed, saveFileRailCollapsed } from "$lib/review/reviewPrefs";
  import type { PrFile } from "$lib/types";
  import { fileIconFor } from "$lib/utils/fileIcons";
  import { loadSize, resizeHandle } from "$lib/utils/resize";

  /** Room kept around the highlighted row: the row above it shows. */
  const RAIL_MARGIN = 32;
  const WIDTH_KEY = "layout.reviewRailWidth";
  const MIN_WIDTH = 160;
  const MAX_WIDTH = 520;

  let {
    files,
    currentPath,
    countsByPath,
    viewed,
    onJump,
    collapsedDirs = $bindable([]),
  }: {
    files: PrFile[];
    currentPath: string | null;
    countsByPath: ReadonlyMap<string, FileCounts>;
    viewed: ReadonlySet<string>;
    onJump: (path: string) => void;
    /** The folded directories' ids. Everything else is open. */
    collapsedDirs?: string[];
  } = $props();

  let width = $state(loadSize(WIDTH_KEY, 240, MIN_WIDTH, MAX_WIDTH));
  let folded = $state(loadFileRailCollapsed());
  let list = $state<HTMLElement>();

  const tree = $derived(buildFileTree(files));
  const collection = $derived(
    createTreeViewCollection<FileTreeNode>({
      nodeToValue: (node) => node.id,
      nodeToString: (node) => node.name,
      rootNode: tree,
    }),
  );
  const dirs = $derived(directoryIds(tree));
  const expanded = $derived.by(() => {
    const shut = new Set(collapsedDirs);
    return dirs.filter((id) => !shut.has(id));
  });
  const anyFolded = $derived(expanded.length < dirs.length);

  function setFolded(value: boolean) {
    folded = value;
    saveFileRailCollapsed(value);
  }

  function onExpandedChange({ expandedValue }: { expandedValue: string[] }) {
    const open = new Set(expandedValue);
    collapsedDirs = dirs.filter((id) => !open.has(id));
  }

  function jumpFrom(event: KeyboardEvent, path: string) {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    onJump(path);
  }

  // The current file is never hidden: reading into a folded directory, or
  // jumping there with `]`, opens it.
  $effect(() => {
    const path = currentPath;
    if (!path) return;
    untrack(() => {
      const above = new Set(ancestorsOf(path));
      if (collapsedDirs.some((id) => above.has(id))) collapsedDirs = collapsedDirs.filter((id) => !above.has(id));
    });
  });

  // Keep the highlighted row in the rail's view. By hand: `scrollIntoView`
  // would also move the page.
  $effect(() => {
    void currentPath;
    void expanded;
    const rail = list;
    if (!rail) return;
    const frame = requestAnimationFrame(() => {
      const row = rail.querySelector('[aria-current="true"]')?.getBoundingClientRect();
      if (!row) return;
      const box = rail.getBoundingClientRect();
      if (row.top - RAIL_MARGIN < box.top) rail.scrollTop -= box.top - row.top + RAIL_MARGIN;
      else if (row.bottom + RAIL_MARGIN > box.bottom) rail.scrollTop += row.bottom - box.bottom + RAIL_MARGIN;
    });
    return () => cancelAnimationFrame(frame);
  });

  /** A file's name tinted by what the PR does to it. */
  function statusClass(status: string) {
    if (status === "added") return "text-success-700-300";
    if (status === "removed") return "text-error-700-300 line-through";
    return "";
  }
</script>

{#snippet treeNode(node: FileTreeNode, indexPath: number[])}
  <TreeView.NodeProvider value={{ node, indexPath }}>
    {#if node.children}
      <TreeView.Branch class="gap-px">
        <TreeView.BranchControl
          class="menu-row gap-1 py-1 pr-2 pl-[calc(var(--depth)*12px-4px)] outline-none focus-visible:ring-1 focus-visible:ring-primary-500 focus-visible:ring-inset"
          title={node.id}
        >
          <TreeView.BranchIndicator class="shrink-0 text-surface-500 transition-transform">
            <ChevronRight size={12} />
          </TreeView.BranchIndicator>
          <TreeView.BranchText class="min-w-0 grow truncate font-mono text-[11px] text-surface-600-400">
            {node.name}
          </TreeView.BranchText>
        </TreeView.BranchControl>
        <TreeView.BranchContent class="gap-px">
          <TreeView.BranchIndentGuide
            class="absolute inset-y-0 left-[calc(var(--depth)*12px+2px)] w-px bg-surface-200-800"
          />
          {#each node.children as child, index (child.id)}
            {@render treeNode(child, [...indexPath, index])}
          {/each}
        </TreeView.BranchContent>
      </TreeView.Branch>
    {:else if node.file}
      {@const file = node.file}
      {@const counts = countsByPath.get(file.path)}
      {@const icon = fileIconFor(node.name)}
      <TreeView.Item
        class="menu-row gap-1.5 py-1 pr-2 pl-[calc(var(--depth)*12px+4px)] outline-none focus-visible:ring-1 focus-visible:ring-primary-500 focus-visible:ring-inset data-selected:bg-surface-200-800 data-selected:text-inherit"
        title={file.oldPath ? `${file.oldPath} → ${file.path}` : file.path}
        onclick={() => onJump(file.path)}
        onkeydown={(event: KeyboardEvent) => jumpFrom(event, file.path)}
      >
        <icon.icon size={12} class="shrink-0 {icon.class}" />
        <span class="min-w-0 grow truncate font-mono text-[11px] {statusClass(file.status)}">{node.name}</span>
        {#if counts?.findings}
          <span class="status-badge preset-tonal-primary" title="Agent findings">{counts.findings}</span>
        {/if}
        {#if counts?.comments}
          <span class="status-badge preset-tonal" title="Comments">{counts.comments}</span>
        {/if}
        {#if viewed.has(file.path)}
          <Check size={12} class="shrink-0 text-success-500" aria-label="Viewed" />
        {/if}
      </TreeView.Item>
    {/if}
  </TreeView.NodeProvider>
{/snippet}

{#if folded}
  <div class="hidden w-10 shrink-0 border-r border-surface-200-800 bg-surface-100-900 @5xl/review:block">
    <div class="sticky top-0 flex flex-col items-center gap-2 py-3">
      <TooltipButton
        type="button"
        label="Expand files"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
        aria-expanded={false}
        onclick={() => setFolded(false)}
      >
        <PanelLeftOpen size={14} aria-hidden="true" />
      </TooltipButton>
      <span class="status-badge preset-tonal" title="Changed files">{files.length}</span>
    </div>
  </div>
{:else}
  <div
    class="relative hidden shrink-0 border-r border-surface-200-800 bg-surface-100-900 @5xl/review:block"
    style="width: {width}px"
  >
    <nav aria-label="Changed files" class="sticky top-0 flex max-h-[100cqh] flex-col py-2">
      <div class="flex shrink-0 items-center gap-1 px-3 pb-1">
        <span class="eyebrow grow">Files <span class="text-surface-500">{files.length}</span></span>
        {#if dirs.length > 0}
          <TooltipButton
            type="button"
            label={anyFolded ? "Expand all folders" : "Collapse all folders"}
            class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
            onclick={() => (collapsedDirs = anyFolded ? [] : [...dirs])}
          >
            {#if anyFolded}
              <ChevronsUpDown size={14} aria-hidden="true" />
            {:else}
              <ChevronsDownUp size={14} aria-hidden="true" />
            {/if}
          </TooltipButton>
        {/if}
        <TooltipButton
          type="button"
          label="Collapse files"
          class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
          aria-expanded={true}
          onclick={() => setFolded(true)}
        >
          <PanelLeftClose size={14} aria-hidden="true" />
        </TooltipButton>
      </div>
      <div bind:this={list} data-rail-scroll class="min-h-0 flex-1 overflow-y-auto px-2">
        <TreeView
          {collection}
          expandedValue={expanded}
          {onExpandedChange}
          selectedValue={currentPath ? [currentPath] : []}
        >
          <TreeView.Tree aria-label="Changed files tree" class="w-full gap-px">
            {#each tree.children ?? [] as node, index (node.id)}
              {@render treeNode(node, [index])}
            {/each}
          </TreeView.Tree>
        </TreeView>
      </div>
      <div class="mx-2 mt-2 flex shrink-0 flex-col gap-1.5 border-t border-surface-200-800 p-2 text-surface-500">
        <div class="flex items-center gap-1.5">
          <kbd class="kbd">]</kbd><kbd class="kbd">[</kbd>Next / previous file
        </div>
        <div class="flex items-center gap-1.5"><kbd class="kbd">v</kbd>Mark viewed and collapse</div>
      </div>
    </nav>
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize files"
      class="absolute inset-y-0 -right-1 z-20 w-2 cursor-col-resize transition-colors hover:bg-primary-500/30 active:bg-primary-500/40"
      use:resizeHandle={{
        axis: "x",
        direction: 1,
        min: MIN_WIDTH,
        max: MAX_WIDTH,
        storageKey: WIDTH_KEY,
        getSize: () => width,
        onResize: (size) => (width = size),
      }}
    ></div>
  </div>
{/if}
