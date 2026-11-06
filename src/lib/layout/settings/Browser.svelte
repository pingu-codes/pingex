<script lang="ts">
  // Browser use: whether new Codex threads may drive the user's own Chrome,
  // which browser to connect to, and the sites allowed in every thread.
  import { X } from "@lucide/svelte";
  import type { BrowserChannel } from "$lib/bindings";
  import { browser, refreshBrowser } from "$lib/browser/browserStore.svelte";
  import HelperInstall from "$lib/browser/HelperInstall.svelte";
  import Toggle from "$lib/components/Toggle.svelte";
  import SettingRow from "$lib/layout/settings/SettingRow.svelte";
  import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { errorMessage } from "$lib/utils/errors";

  let error = $state<string | null>(null);
  const overview = $derived(browser.overview);

  $effect(() => {
    refreshBrowser().catch((cause) => (error = errorMessage(cause)));
  });

  const CHANNELS: { value: BrowserChannel | ""; label: string }[] = [
    { value: "", label: "Automatic (Chrome first)" },
    { value: "chrome", label: "Chrome" },
    { value: "chromeBeta", label: "Chrome Beta" },
    { value: "chromeCanary", label: "Chrome Canary" },
    { value: "chromium", label: "Chromium" },
    { value: "brave", label: "Brave" },
    { value: "edge", label: "Edge" },
  ];

  async function save(enabled: boolean, channel: BrowserChannel | null) {
    error = null;
    try {
      await commands.browserSetSettings(enabled, channel);
      await refreshBrowser();
    } catch (cause) {
      error = errorMessage(cause);
    }
  }

  async function untrust(site: string) {
    try {
      await commands.browserRevokeSite(null, site);
      await refreshBrowser();
    } catch (cause) {
      error = errorMessage(cause);
    }
  }
</script>

<SettingsCard
  title="Browser"
  description="Let Codex drive the browser you already use: your profile, sign-ins and tabs. Separate from shell and file permissions."
>
  {#if error}
    <p role="alert" class="text-xs text-error-500">{error}</p>
  {/if}
  <div class="mt-1">
    <SettingRow
      label="Browser use"
      description="New Codex threads get browser tools. The agent only works in tabs it opens or you hand over, kept in a Pingex tab group, and asks before visiting each new site."
      local
    >
      <Toggle
        checked={overview?.enabled ?? false}
        label="Browser use"
        testId="browser-use-switch"
        disabled={!overview}
        onChange={(checked) => save(checked, overview?.channel ?? null)}
      />
    </SettingRow>
    <SettingRow label="Browser" description="Which running browser Pingex connects to." local>
      <select
        class="select w-48 text-sm"
        aria-label="Browser"
        value={overview?.channel ?? ""}
        disabled={!overview}
        onchange={(event) =>
          save(overview?.enabled ?? false, (event.currentTarget.value || null) as BrowserChannel | null)}
      >
        {#each CHANNELS as channel (channel.value)}
          <option value={channel.value}>{channel.label}</option>
        {/each}
      </select>
    </SettingRow>
    <div class="space-y-1.5 py-3">
      <div class="text-sm">Always allowed sites</div>
      {#if overview && overview.trustedSites.length > 0}
        <div class="flex flex-wrap gap-1.5">
          {#each overview.trustedSites as site (site)}
            <span class="badge preset-tonal-surface gap-1 font-mono">
              {site}
              <button aria-label="Stop always allowing {site}" onclick={() => untrust(site)}><X size={10} /></button>
            </span>
          {/each}
        </div>
      {:else}
        <p class="text-xs text-surface-500">Sites you choose "Always allow" for show up here.</p>
      {/if}
    </div>
  </div>
  <div class="space-y-1.5 text-[11px] leading-4 text-surface-500">
    <p>
      Setup: in Chrome 144 or newer open <span class="font-mono">chrome://inspect/#remote-debugging</span> and allow
      remote debugging. Chrome asks you to allow each connection from Pingex.
    </p>
    <p>
      For tab groups, install the Pingex Browser Helper
      {#if overview}
        (id <span class="font-mono">{overview.helperExtensionId}</span>)
      {/if}
      :
    </p>
    <HelperInstall dir={overview?.helperDir} />
  </div>
</SettingsCard>
