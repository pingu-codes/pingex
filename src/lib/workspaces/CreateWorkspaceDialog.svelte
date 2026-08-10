<script lang="ts">
import { FolderGit2, Layers3 } from "@lucide/svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import type { CreateWorkspaceInput, Project, WorkspaceMemberInput } from "$lib/types";

let {
  projects,
  workspace = null,
  submit: save,
  close,
}: {
  projects: Project[];
  /** The workspace being edited, or null to create a new one. */
  workspace?: Project | null;
  /** Persists the workspace; rejecting keeps the dialog open with the error. */
  submit: (input: CreateWorkspaceInput) => Promise<void>;
  close: DialogClose<true>;
} = $props();

// The dialog is mounted per opening, so the editing seed is read once here
// rather than tracked by an effect.
// svelte-ignore state_referenced_locally
const seed = workspace;
const members = seed?.members ?? [];

let name = $state(seed?.name ?? "");
let isolateGit = $state(members.some((member) => member.isolated) || members.length === 0);
let selected = $state<Record<string, boolean>>(Object.fromEntries(members.map((member) => [member.sourcePath, true])));
let aliases = $state<Record<string, string>>(
  Object.fromEntries(members.map((member) => [member.sourcePath, member.alias])),
);
let isolated = $state<Record<string, boolean>>(
  Object.fromEntries(members.map((member) => [member.sourcePath, member.isolated])),
);
const action = submitState();

const choices = $derived(projects.filter((project) => !project.archived && project.kind !== "multiProject"));
const selectedChoices = $derived(choices.filter((project) => selected[project.path]));
const canCreate = $derived(name.trim().length > 0 && selectedChoices.length >= 2 && !action.busy);

function basename(path: string) {
  return path.split("/").filter(Boolean).at(-1) || "project";
}

function defaultAlias(project: Project) {
  const base = basename(project.path).replace(/[^A-Za-z0-9._-]+/g, "-") || "project";
  const used = new Set(
    Object.entries(aliases)
      .filter(([path]) => path !== project.path)
      .map(([, value]) => value),
  );
  if (!used.has(base)) return base;
  let index = 2;
  while (used.has(`${base}-${index}`)) index += 1;
  return `${base}-${index}`;
}

function toggle(project: Project) {
  const nowSelected = !selected[project.path];
  selected[project.path] = nowSelected;
  if (nowSelected) {
    aliases[project.path] ||= defaultAlias(project);
    isolated[project.path] ??= isolateGit;
  }
}

function memberInput(project: Project): WorkspaceMemberInput {
  return {
    sourcePath: project.path,
    alias: aliases[project.path]?.trim() || defaultAlias(project),
    isolated: isolated[project.path] ?? isolateGit,
  };
}

async function confirm() {
  if (!canCreate) return;
  const input = { name: name.trim(), members: selectedChoices.map(memberInput) };
  if (await action.run(() => save(input))) close(true);
}
</script>

<DialogShell
  title={seed ? "Edit workspace" : "New workspace"}
  subtitle="A writable shared parent with links to several projects."
  width={672}
  onClose={() => close()}
>
  {#snippet icon()}<Layers3 size={17} class="text-primary-500" />{/snippet}

  <label class="mt-4 block text-xs font-medium">
    Workspace name
    <input bind:value={name} placeholder="Frontend + API" class="input mt-1.5 w-full" />
  </label>

  <label class="mt-4 flex items-start gap-2.5 rounded-lg border border-surface-200-800 p-3 text-xs">
    <input
      type="checkbox"
      checked={isolateGit}
      onchange={(event) => {
        isolateGit = event.currentTarget.checked;
        for (const project of selectedChoices) isolated[project.path] = isolateGit;
      }}
      class="mt-0.5"
    />
    <span><span class="font-medium">Create isolated worktrees for Git projects</span><span class="mt-0.5 block leading-5 text-surface-500">On by default, so this workspace does not alter your existing checkout. Non-Git folders remain direct.</span></span>
  </label>

  <div class="mt-4 max-h-72 space-y-1 overflow-y-auto rounded-lg border border-surface-200-800 p-1.5">
    {#if choices.length === 0}
      <p class="p-3 text-xs text-surface-500">Add at least two project folders before creating a workspace.</p>
    {:else}
      {#each choices as project (project.path)}
        <div class="rounded-md px-2 py-2 hover:bg-surface-100-900">
          <label class="flex cursor-pointer items-center gap-2">
            <input type="checkbox" checked={!!selected[project.path]} onchange={() => toggle(project)} />
            <FolderGit2 size={14} class="shrink-0 text-surface-500" />
            <span class="min-w-0 flex-1"><span class="block truncate text-xs font-medium">{project.name}</span><span class="block truncate text-[10px] text-surface-500">{project.path}</span></span>
          </label>
          {#if selected[project.path]}
            <div class="ml-7 mt-2 grid grid-cols-[minmax(0,1fr)_auto] items-center gap-2">
              <input aria-label={`Alias for ${project.name}`} bind:value={aliases[project.path]} class="input h-8 min-w-0 text-xs" placeholder="alias" />
              <label class="flex items-center gap-1.5 whitespace-nowrap text-[11px] text-surface-600-400">
                <input type="checkbox" bind:checked={isolated[project.path]} /> Isolated worktree
              </label>
            </div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  {#if action.error}<p class="mt-3 text-xs text-error-500">{action.error}</p>{/if}

  {#snippet footer()}
    <span class="mr-auto text-[11px] text-surface-500">{selectedChoices.length} of at least 2 projects selected</span>
    <button class="btn btn-sm" onclick={() => close()} disabled={action.busy}>Cancel</button>
    <button class="btn btn-sm preset-filled-primary-500" onclick={confirm} disabled={!canCreate}>
      {action.busy ? "Saving…" : seed ? "Save workspace" : "Create workspace"}
    </button>
  {/snippet}
</DialogShell>
