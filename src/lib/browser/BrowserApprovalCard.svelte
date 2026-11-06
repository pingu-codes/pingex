<script lang="ts">
  // A question Pingex asks on the browser's behalf. The agent's tool call
  // waits on the answer.
  import { Globe } from "@lucide/svelte";
  import type { BrowserApproval, BrowserDecision } from "$lib/bindings";
  import { resolveBrowserApproval } from "$lib/browser/browserStore.svelte";
  import { submitState } from "$lib/utils/actionState.svelte";

  let { approval }: { approval: BrowserApproval } = $props();

  const action = submitState();

  const title = $derived.by(() => {
    switch (approval.kind) {
      case "site":
        return `Codex wants to visit ${approval.site ?? "a new site"}`;
      case "takeTab":
        return "Codex wants to take over one of your tabs";
      case "upload":
        return "Codex wants to upload files";
      case "fullAccess":
        return "Codex wants full access to your browser";
    }
  });

  async function decide(decision: BrowserDecision) {
    await action.run(() => resolveBrowserApproval(approval.id, decision));
  }
</script>

<div class="card preset-tonal-warning space-y-2.5 p-3 text-sm" data-testid="browser-approval">
  {#if action.error}
    <p role="alert" class="text-error-500">{action.error}</p>
  {/if}
  <div class="flex items-center gap-2 text-xs font-semibold">
    <Globe size={14} />
    {title}
  </div>
  {#if approval.tabTitle || approval.url}
    <div class="rounded-lg bg-surface-950/80 px-3 py-2 font-mono text-[11px] leading-5 text-surface-50">
      {#if approval.tabTitle}
        <div class="truncate">{approval.tabTitle}</div>
      {/if}
      {#if approval.url}
        <div class="truncate opacity-80">{approval.url}</div>
      {/if}
    </div>
  {/if}
  <p class="text-xs opacity-80">{approval.detail}</p>
  {#if approval.kind === "fullAccess"}
    <p class="text-xs opacity-80">
      This lets the agent see every tab, read every site's cookies and send commands to the browser itself, for the rest
      of this thread.
    </p>
  {/if}
  <div class="flex flex-wrap gap-2 pt-0.5">
    <button onclick={() => decide("allow")} disabled={action.busy} class="btn btn-sm preset-filled-primary-500">
      Allow
    </button>
    {#if approval.kind === "site"}
      <button onclick={() => decide("alwaysAllow")} disabled={action.busy} class="btn btn-sm preset-tonal">
        Always allow
      </button>
    {/if}
    <button onclick={() => decide("deny")} disabled={action.busy} class="btn btn-sm preset-tonal">Deny</button>
  </div>
</div>
