<script lang="ts" module>
  /** The user-initiated network operations on the Git tab. */
  export type SyncOperation = "fetch" | "pull" | "push" | "publish";
</script>

<script lang="ts">
  import { ArrowDownToLine, ArrowUpFromLine, CloudDownload } from "@lucide/svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";

  let {
    upstream = null,
    ahead = 0,
    behind = 0,
    detached = false,
    busy = null,
    prNumber = null,
    onSync,
  }: {
    upstream?: string | null;
    ahead?: number;
    behind?: number;
    detached?: boolean;
    /** The operation currently running, if any. */
    busy?: SyncOperation | null;
    /** This checkout is a managed PR worktree: its branch has no upstream to
     *  publish, and its commits go to this PR instead. */
    prNumber?: number | null;
    onSync: (operation: SyncOperation) => void;
  } = $props();

  const disabled = $derived(busy !== null);
</script>

<div class="flex items-center gap-1" role="group" aria-label="Sync">
  <TooltipButton
    label="Fetch from remote"
    onclick={() => onSync("fetch")}
    {disabled}
    aria-label="Fetch"
    class="btn btn-sm preset-tonal disabled:opacity-40"
  >
    <CloudDownload size={13} class={busy === "fetch" ? "animate-pulse" : ""} />
    Fetch
  </TooltipButton>
  {#if upstream}
    <TooltipButton
      label="Pull (fast-forward only)"
      onclick={() => onSync("pull")}
      {disabled}
      aria-label="Pull"
      class="btn btn-sm preset-tonal disabled:opacity-40"
    >
      <ArrowDownToLine size={13} class={busy === "pull" ? "animate-pulse" : ""} />
      Pull
      {#if behind > 0}
        <span class="font-mono text-[10px]">↓{behind}</span>
      {/if}
    </TooltipButton>
    <TooltipButton
      label="Push to {upstream}"
      onclick={() => onSync("push")}
      {disabled}
      aria-label="Push"
      class="btn btn-sm {ahead > 0 ? "preset-filled-primary-500" : "preset-tonal"} disabled:opacity-40"
    >
      <ArrowUpFromLine size={13} class={busy === "push" ? "animate-pulse" : ""} />
      Push
      {#if ahead > 0}
        <span class="font-mono text-[10px]">↑{ahead}</span>
      {/if}
    </TooltipButton>
  {:else if !detached}
    <TooltipButton
      label={prNumber === null ? "Push and set origin as upstream" : `Push these commits to #${prNumber}`}
      onclick={() => onSync("publish")}
      {disabled}
      aria-label={prNumber === null ? "Publish branch" : "Push to PR"}
      class="btn btn-sm preset-filled-primary-500 disabled:opacity-40"
    >
      <ArrowUpFromLine size={13} class={busy === "publish" ? "animate-pulse" : ""} />
      {prNumber === null ? "Publish branch" : "Push to PR"}
    </TooltipButton>
  {/if}
</div>
