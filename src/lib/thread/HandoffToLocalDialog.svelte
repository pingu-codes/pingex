<script lang="ts">
import { ArrowLeftRight, ChevronDown, RefreshCw } from "@lucide/svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";
import { gitWorktreeHandoffPreflight } from "$lib/services/api";
import { dirName } from "$lib/thread/handoff";
import type { WorktreeHandoffPreflight } from "$lib/types";

let {
  worktreePath,
  targets,
  defaultTarget,
  submit,
  close,
}: {
  /** The temporary worktree the thread runs in. */
  worktreePath: string;
  /** Local workspaces the branch may be checked out into (repo folders). */
  targets: { path: string; name: string }[];
  defaultTarget: string;
  /** Performs the handoff; rejecting keeps the dialog open. */
  submit: (targetDir: string, commitUncommitted: boolean) => Promise<void>;
  close: DialogClose<true>;
} = $props();

// svelte-ignore state_referenced_locally
let target = $state(defaultTarget);
let preflight = $state<WorktreeHandoffPreflight | null>(null);
let checking = $state(true);
let checkError = $state<string | null>(null);
let commitUncommitted = $state(true);
const action = submitState();

const options = $derived(
  targets.some((entry) => entry.path === defaultTarget)
    ? targets
    : [{ path: defaultTarget, name: dirName(defaultTarget) }, ...targets],
);

let request = 0;
$effect(() => {
  const dir = target;
  const id = ++request;
  checking = true;
  checkError = null;
  gitWorktreeHandoffPreflight(worktreePath, dir)
    .then((result) => {
      if (id === request) preflight = result;
    })
    .catch((cause) => {
      if (id === request) checkError = cause instanceof Error ? cause.message : String(cause);
    })
    .finally(() => {
      if (id === request) checking = false;
    });
});

const blocker = $derived(
  checkError ??
    preflight?.blocker ??
    (preflight?.worktreeDirty && !commitUncommitted ? "Commit the worktree's changes to hand off" : null),
);
const ready = $derived(!checking && !blocker && !!preflight?.branch);

async function handoff() {
  if (!ready) return;
  if (await action.run(() => submit(target, commitUncommitted))) close(true);
}
</script>

<DialogShell title="Hand off chat to local" width={480} onClose={() => close()}>
  <div class="mt-3 grid size-10 place-items-center rounded-xl bg-surface-200-800">
    <ArrowLeftRight size={18} class="text-surface-700-300" />
  </div>
  <p class="mt-3 text-sm leading-7 text-surface-600-400">
    Check out branch
    <span class="rounded-lg bg-surface-200-800 px-2 py-1 font-mono text-xs text-surface-900-100">
      {preflight?.branch ?? "…"}
    </span>
    in a local workspace and detach it from worktree.
  </p>

  <label class="mt-4 block text-sm text-surface-600-400" for="handoff-target">Handing off to local workspace</label>
  <div class="relative mt-2">
    <select
      id="handoff-target"
      bind:value={target}
      class="select w-full appearance-none rounded-lg bg-surface-200-800 px-3 py-2 pr-8 text-sm"
    >
      {#each options as option (option.path)}
        <option value={option.path}>{option.name}</option>
      {/each}
    </select>
    <ChevronDown size={14} class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-surface-500" />
  </div>
  <p class="mt-1 truncate text-xs text-surface-500" title={target}>{target}</p>

  {#if preflight?.worktreeDirty}
    <label class="mt-3 flex items-center gap-2 text-xs text-surface-600-400">
      <input type="checkbox" class="checkbox" bind:checked={commitUncommitted} />
      Commit the worktree's uncommitted changes first
    </label>
  {/if}

  {#if action.error}
    <div class="card preset-tonal-error mt-3 p-3 text-xs">{action.error}</div>
  {/if}

  {#snippet footer()}
    <div class="flex w-full flex-col items-center gap-2">
      <button
        type="button"
        disabled={!ready || action.busy}
        onclick={handoff}
        class="btn w-full preset-filled-primary-500 disabled:opacity-50"
      >
        {#if checking}<RefreshCw size={14} class="animate-spin" />{/if}
        {action.busy ? "Handing off…" : "Hand off"}
      </button>
      {#if blocker && !checking}
        <p class="text-center text-sm text-error-500">{blocker}</p>
      {/if}
    </div>
  {/snippet}
</DialogShell>
