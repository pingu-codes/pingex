<script lang="ts">
  // The thread's view of the user's browser: whether Pingex is connected,
  // which tabs the thread owns, what it may visit, and a Stop button.
  import { Copy, Crosshair, Square, Undo2, X } from "@lucide/svelte";
  import type { ConnectionState } from "$lib/bindings";
  import { browser, nativeThreadId, refreshBrowser, threadViews } from "$lib/browser/browserStore.svelte";
  import HelperInstall from "$lib/browser/HelperInstall.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { openSettings } from "$lib/services/settingsNav.svelte";
  import { submitState } from "$lib/utils/actionState.svelte";

  let { threadId = null }: { threadId?: string | null } = $props();

  const native = $derived(threadId ? nativeThreadId(threadId) : null);
  const overview = $derived(browser.overview);
  const status = $derived(overview?.status ?? null);
  const view = $derived(native ? threadViews.get(native) : undefined);
  const action = submitState();

  $effect(() => {
    void refreshBrowser(threadId).catch((cause) => (action.error = String(cause)));
  });

  const STATE_LABELS: Record<ConnectionState, string> = {
    idle: "Not connected yet",
    notFound: "No browser found",
    awaitingApproval: "Waiting for you to allow the connection",
    connected: "Connected",
    lost: "Connection lost",
    failed: "Not connected",
  };
  const DEBUG_URL = "chrome://inspect/#remote-debugging";

  const connect = () => action.run(async () => void (await commands.browserConnect()));
  const release = (targetId: string) =>
    native ? action.run(() => commands.browserReleaseTab(native, targetId)) : undefined;
  const focus = (tabId: number) => action.run(() => commands.browserFocusTab(tabId));
  const revoke = (site: string) =>
    action.run(async () => {
      await commands.browserRevokeSite(native, site);
      await refreshBrowser(threadId);
    });
  const revokeTrusted = (site: string) =>
    action.run(async () => {
      await commands.browserRevokeSite(null, site);
      await refreshBrowser(threadId);
    });
  const dropFullAccess = () => (native ? action.run(() => commands.browserSetFullAccess(native, false)) : undefined);
  const stop = () => (native ? action.run(() => commands.browserStop(native)) : undefined);
</script>

{#snippet heading(text: string)}
  <h3 class="mb-1.5 text-[10px] font-semibold uppercase tracking-wide text-surface-500">{text}</h3>
{/snippet}

<div class="space-y-4 text-xs" data-testid="browser-panel">
  {#if action.error}
    <p role="alert" class="text-error-500">{action.error}</p>
  {/if}

  {#if overview && !overview.enabled}
    <div class="card preset-tonal-surface space-y-2 p-3">
      <p>Browser use is off. New Codex threads get the browser tools once it is on.</p>
      <button class="btn btn-sm preset-filled-primary-500" onclick={() => openSettings("browser")}>
        Open Browser settings
      </button>
    </div>
  {/if}

  <section>
    {@render heading("Connection")}
    <div class="space-y-2">
      <div class="flex items-center gap-2">
        <span
          class="size-2 shrink-0 rounded-full {status?.state === "connected"
            ? "bg-success-500"
            : status?.state === "awaitingApproval"
              ? "bg-warning-500"
              : "bg-surface-400-600"}"
        ></span>
        <span class="flex-1">
          {status ? STATE_LABELS[status.state] : "…"}{status?.browser ? ` · ${status.browser}` : ""}
        </span>
        {#if status && status.state !== "connected" && status.state !== "awaitingApproval"}
          <button class="btn btn-sm preset-tonal" disabled={action.busy} onclick={connect}>Connect</button>
        {/if}
      </div>
      {#if status?.message}
        <p class="text-surface-600-400">{status.message}</p>
      {/if}
      {#if status && status.state !== "connected"}
        <ol class="list-decimal space-y-1 pl-4 text-surface-600-400">
          <li>Open Chrome 144 or newer, the browser you normally use.</li>
          <li>
            Go to
            <button
              class="inline-flex items-center gap-1 font-mono text-primary-500 hover:underline"
              onclick={() => commands.copyText(DEBUG_URL)}
              title="Copy"
            >
              {DEBUG_URL}
              <Copy size={10} />
            </button>
            and allow remote debugging.
          </li>
          <li>Click Connect, then Allow in Chrome's dialog.</li>
        </ol>
      {/if}
      {#if status?.state === "connected" && !status.helper}
        <p class="text-surface-600-400">
          The Pingex Browser Helper keeps this thread's tabs in their own tab group. Without it, tabs still work but are
          not grouped.
        </p>
        <HelperInstall dir={overview?.helperDir} />
      {/if}
    </div>
  </section>

  {#if native}
    <section>
      {@render heading("This thread's tabs")}
      {#if !view || view.tabs.length === 0}
        <p class="text-surface-600-400">None yet. The agent opens tabs, or asks to take over one of yours.</p>
      {:else}
        <ul class="space-y-1.5">
          {#each view.tabs as tab (tab.targetId)}
            <li class="flex items-center gap-2">
              <span class="shrink-0 rounded bg-surface-200-800 px-1 font-mono text-[10px]">{tab.id}</span>
              <span class="min-w-0 flex-1">
                <span class="block truncate">{tab.title || "Untitled"}</span>
                <span class="block truncate text-[10px] text-surface-500">{tab.url}</span>
              </span>
              {#if tab.chromeTabId != null && status?.helper}
                <button
                  class="btn-icon btn-icon-sm preset-tonal"
                  title="Show in Chrome"
                  aria-label="Show {tab.id} in Chrome"
                  onclick={() => focus(tab.chromeTabId as number)}
                >
                  <Crosshair size={12} />
                </button>
              {/if}
              <button
                class="btn-icon btn-icon-sm preset-tonal"
                title="Take it back (leaves it open)"
                aria-label="Take back {tab.id}"
                onclick={() => release(tab.targetId)}
              >
                <Undo2 size={12} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      {@render heading("Allowed sites")}
      {#if view?.fullAccess}
        <div class="mb-2 flex items-center gap-2">
          <span class="badge preset-tonal-warning">Full browser access</span>
          <button class="text-[10px] text-surface-500 hover:text-error-500" onclick={dropFullAccess}>Revoke</button>
        </div>
      {/if}
      {#if !view || view.sites.length === 0}
        <p class="text-surface-600-400">None for this thread yet; the first visit to a site asks.</p>
      {:else}
        <ul class="flex flex-wrap gap-1.5">
          {#each view.sites as site (site)}
            <li class="badge preset-tonal-surface gap-1 font-mono">
              {site}
              <button aria-label="Revoke {site}" onclick={() => revoke(site)}><X size={10} /></button>
            </li>
          {/each}
        </ul>
      {/if}
      {#if overview && overview.trustedSites.length > 0}
        <p class="mt-2 text-surface-600-400">Always allowed in every thread:</p>
        <ul class="mt-1 flex flex-wrap gap-1.5">
          {#each overview.trustedSites as site (site)}
            <li class="badge preset-tonal-surface gap-1 font-mono">
              {site}
              <button aria-label="Stop always allowing {site}" onclick={() => revokeTrusted(site)}>
                <X size={10} />
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <button class="btn btn-sm preset-tonal-error w-full" disabled={action.busy} onclick={stop}>
      <Square size={12} />
      Stop browser actions
    </button>
  {/if}
</div>
