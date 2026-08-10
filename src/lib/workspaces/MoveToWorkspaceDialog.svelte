<script lang="ts">
import { Layers3 } from "@lucide/svelte";
import type { DialogClose } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import type { Project } from "$lib/types";

let {
  workspaces,
  close,
}: {
  workspaces: Project[];
  /** Resolves the workspace to move the thread into. */
  close: DialogClose<Project>;
} = $props();

const choices = $derived(workspaces.filter((project) => project.kind === "multiProject" && !project.archived));
</script>

<DialogShell
  title="Move to workspace"
  subtitle="The next turn will start in the workspace hub and use all of its member roots."
  width={448}
  onClose={() => close()}
>
  {#snippet icon()}<Layers3 size={17} class="text-primary-500" />{/snippet}

  <div class="mt-4 space-y-1">
    {#each choices as workspace (workspace.path)}
      <button
        class="flex w-full items-center gap-3 rounded-lg border border-surface-200-800 px-3 py-2.5 text-left text-xs hover:preset-tonal"
        onclick={() => close(workspace)}
      >
        <Layers3 size={15} class="shrink-0 text-primary-500" />
        <span class="min-w-0 flex-1"><span class="block truncate font-medium">{workspace.name}</span><span class="block truncate text-[10px] text-surface-500">{workspace.members?.map((member) => member.alias).join(" · ")}</span></span>
      </button>
    {:else}
      <p class="rounded-lg border border-dashed border-surface-300-700 p-4 text-xs text-surface-500">Create a workspace before moving this thread.</p>
    {/each}
  </div>
</DialogShell>
