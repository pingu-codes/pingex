<script lang="ts">
import { ChevronRight, Folder, FolderOpen } from "@lucide/svelte";
import { listProjectFiles } from "$lib/services/api";
import { fileIconFor } from "$lib/utils/fileIcons";

let {
  root,
  onOpenFile,
}: {
  root: string;
  onOpenFile: (relativePath: string) => void;
} = $props();

interface TreeNode {
  name: string;
  path: string;
  children: TreeNode[] | null;
}

let paths = $state<string[] | null>(null);
let error = $state<string | null>(null);
let expanded = $state<Record<string, boolean>>({});

$effect(() => {
  const target = root;
  paths = null;
  error = null;
  expanded = {};
  listProjectFiles(target)
    .then((files) => {
      if (target === root) paths = files;
    })
    .catch((cause) => {
      if (target === root) error = cause instanceof Error ? cause.message : String(cause);
    });
});

const tree = $derived.by(() => {
  const rootNodes: TreeNode[] = [];
  const dirs = new Map<string, TreeNode>();
  for (const filePath of paths ?? []) {
    const segments = filePath.split("/");
    let siblings = rootNodes;
    let prefix = "";
    for (let index = 0; index < segments.length - 1; index++) {
      prefix = prefix ? `${prefix}/${segments[index]}` : segments[index];
      let dir = dirs.get(prefix);
      if (!dir) {
        dir = { name: segments[index], path: prefix, children: [] };
        dirs.set(prefix, dir);
        siblings.push(dir);
      }
      siblings = dir.children!;
    }
    siblings.push({ name: segments[segments.length - 1], path: filePath, children: null });
  }
  const sortNodes = (nodes: TreeNode[]) => {
    nodes.sort((a, b) => Number(b.children !== null) - Number(a.children !== null) || a.name.localeCompare(b.name));
    for (const node of nodes) if (node.children) sortNodes(node.children);
  };
  sortNodes(rootNodes);
  return rootNodes;
});
</script>

{#snippet node(entry: TreeNode, depth: number)}
  {#if entry.children}
    <button
      onclick={() => (expanded[entry.path] = !expanded[entry.path])}
      style={`padding-left: ${depth * 14 + 8}px`}
      class="flex w-full items-center gap-1.5 rounded py-1 pr-2 text-left text-xs hover:preset-tonal"
    >
      <ChevronRight size={11} class="shrink-0 text-surface-500 transition {expanded[entry.path] ? 'rotate-90' : ''}" />
      {#if expanded[entry.path]}
        <FolderOpen size={13} class="shrink-0 text-surface-500" />
      {:else}
        <Folder size={13} class="shrink-0 text-surface-500" />
      {/if}
      <span class="min-w-0 flex-1 truncate">{entry.name}</span>
    </button>
    {#if expanded[entry.path]}
      {#each entry.children as child (child.path)}
        {@render node(child, depth + 1)}
      {/each}
    {/if}
  {:else}
    {@const icon = fileIconFor(entry.name)}
    <button
      onclick={() => onOpenFile(entry.path)}
      title={entry.path}
      style={`padding-left: ${depth * 14 + 8 + 16}px`}
      class="flex w-full items-center gap-1.5 rounded py-1 pr-2 text-left text-xs hover:preset-tonal"
    >
      <icon.icon size={13} class="shrink-0 {icon.class}" />
      <span class="min-w-0 flex-1 truncate">{entry.name}</span>
    </button>
  {/if}
{/snippet}

{#if error}
  <div class="card preset-tonal-error px-3 py-2 text-xs">{error}</div>
{:else if paths === null}
  <div class="space-y-2" aria-label="Loading files">
    <div class="placeholder h-6 animate-pulse rounded-lg"></div>
    <div class="placeholder h-6 animate-pulse rounded-lg opacity-70"></div>
    <div class="placeholder h-6 animate-pulse rounded-lg opacity-40"></div>
  </div>
{:else if tree.length === 0}
  <p class="text-xs text-surface-500">No files in this project.</p>
{:else}
  {#each tree as entry (entry.path)}
    {@render node(entry, 0)}
  {/each}
{/if}
