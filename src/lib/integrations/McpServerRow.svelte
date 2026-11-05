<!--
  One MCP server: what config.toml declares joined with what Codex reports
  live, its controls, and (folded) its tools, resources and server info.
-->
<script lang="ts">
  import { ChevronRight, ExternalLink, LogIn, Plug, X } from "@lucide/svelte";
  import { Collapsible } from "@skeletonlabs/skeleton-svelte";
  import ConfirmDeleteButton from "$lib/integrations/ConfirmDeleteButton.svelte";
  import {
    authAction,
    capabilitySummary,
    contributionSummary,
    type OverrideValue,
    overrideValue,
    pluginNote,
    pluginServerLabel,
    resourcesOf,
    resourceTemplatesOf,
    rowStatus,
    runtimeStatusLabel,
    serverInfoLines,
    statusDotClass,
    statusLabel,
    toolParameters,
    toolsOf,
  } from "$lib/integrations/integrationsHelpers";
  import OverrideSelect from "$lib/integrations/OverrideSelect.svelte";
  import { commands } from "$lib/services/homeRouting";
  import type { IntegrationSetting, McpServerStatus, McpServerSummary } from "$lib/types";

  let {
    server,
    live,
    setting,
    ownerEnabled = true,
    projectPath = null,
    checking = false,
    busy = false,
    open = false,
    signingIn = false,
    onOpenChange,
    onToggle,
    onEdit,
    onRemove,
    onSignIn,
    onSetting,
  }: {
    server: McpServerSummary;
    /** Codex's live view of the server, when it has reported one. */
    live: McpServerStatus | undefined;
    setting: IntegrationSetting | undefined;
    /** False when the plugin that provides this server is turned off. */
    ownerEnabled?: boolean;
    /** Set when showing a project's overrides rather than the home's config. */
    projectPath?: string | null;
    checking?: boolean;
    busy?: boolean;
    /** Whether the details are expanded. */
    open?: boolean;
    /** An OAuth sign-in is in flight. */
    signingIn?: boolean;
    onOpenChange: (open: boolean) => void;
    onToggle: () => void;
    onEdit: () => void;
    onRemove: () => Promise<unknown>;
    onSignIn: () => void;
    onSetting: (value: OverrideValue) => void;
  } = $props();

  const status = $derived(rowStatus(server, live, checking));
  const tools = $derived(toolsOf(live));
  const uris = $derived([
    { title: "Resources", rows: resourcesOf(live) },
    {
      title: "Resource templates",
      rows: resourceTemplatesOf(live).map((template) => ({ ...template, uri: template.uriTemplate })),
    },
  ]);
  const info = $derived(serverInfoLines(live));
  const contribution = $derived(contributionSummary(live));
  const hasDetails = $derived(tools.length + uris[0].rows.length + uris[1].rows.length + info.length > 0);
  const auth = $derived(authAction(live));
  const editable = $derived(server.scope === "user" || server.scope === "global");
  const stateLabel = $derived(
    server.scope === "plugin" && !live
      ? pluginServerLabel(server, setting, ownerEnabled)
      : projectPath
        ? server.enabled
          ? "Enabled"
          : "Disabled"
        : statusLabel(status),
  );
  const runtime = $derived(runtimeStatusLabel(live));
  const plugin = $derived(pluginNote(setting, ownerEnabled));
</script>

<div id="mcp-row-{server.name}" class="card border border-surface-200-800 bg-surface-50-950 p-3">
  <Collapsible {open} onOpenChange={(details) => onOpenChange(details.open)} class="items-stretch gap-0">
    <div class="flex items-center gap-3">
      <Plug size={16} class="shrink-0 text-surface-500" />
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2">
          <span class="truncate font-medium">{live?.serverInfo?.title || server.name}</span>
          <span class="shrink-0 rounded-full bg-surface-200-800 px-1.5 py-0.5 text-[10px] text-surface-600-400"
            >{server.scope}</span
          >
        </div>
        <div class="mt-0.5 truncate text-[11px] text-surface-500">{capabilitySummary(server, live)}</div>
      </div>
      <div class="flex shrink-0 items-center gap-1.5">
        <span class="size-1.5 rounded-full {statusDotClass(status)}"></span>
        <span class="text-[10px] text-surface-500">{stateLabel}</span>
        {#if runtime && runtime !== statusLabel(status)}
          <span class="text-[10px] text-surface-500" title="Codex runtime status">· {runtime}</span>
        {/if}
      </div>
    </div>

    {#if live?.error}
      <div class="mt-2 flex items-start gap-1.5 text-[11px] text-error-500">
        <X size={12} class="mt-0.5 shrink-0" />
        <span>{live.error}</span>
      </div>
    {/if}
    {#if auth === "signIn"}
      <div class="mt-2 text-[11px] text-warning-600-400">
        This server needs you to sign in before its tools are available.
      </div>
    {:else if auth === "env"}
      <div class="mt-2 text-[11px] text-surface-500">
        Authenticated from <span class="font-mono">{server.bearerTokenEnvVar ?? "a bearer token"}</span> in your
        environment.
      </div>
    {/if}

    <div class="mt-2.5 flex flex-wrap items-center gap-1.5">
      {#if auth === "signIn" && !projectPath}
        <button
          type="button"
          onclick={onSignIn}
          disabled={signingIn}
          class="btn btn-sm preset-filled-primary-500 text-xs"
        >
          <LogIn size={12} />
          {signingIn ? "Waiting for browser…" : "Sign in"}
        </button>
      {:else if auth === "signedIn"}
        <span class="text-[11px] text-success-500">Signed in</span>
      {/if}
      {#if projectPath}
        <OverrideSelect
          label="Setting for {server.name}"
          value={overrideValue(setting)}
          inheritLabel={server.scope === "plugin"
            ? "Inherit"
            : `Inherit (${setting?.inheritedEnabled ? "enabled" : "disabled"})`}
          disabled={busy}
          onChange={onSetting}
        />
      {:else}
        <button type="button" onclick={onToggle} class="btn btn-sm hover:preset-tonal text-xs">
          {server.enabled ? "Disable" : "Enable"}
        </button>
        {#if editable}
          <button type="button" onclick={onEdit} class="btn btn-sm hover:preset-tonal text-xs">Edit</button>
        {/if}
      {/if}
      {#if hasDetails}
        <Collapsible.Trigger class="group btn btn-sm hover:preset-tonal text-xs">
          <ChevronRight size={12} class="transition-transform group-data-[state=open]:rotate-90" />
          {contribution || "Details"}
        </Collapsible.Trigger>
      {/if}
      {#if plugin}
        <span class="text-xs text-surface-500">{plugin}</span>
      {/if}
      {#if !projectPath && editable}
        <ConfirmDeleteButton label="Remove {server.name}" confirmLabel="Confirm remove" onConfirm={onRemove} />
      {/if}
    </div>

    {#if hasDetails}
      <Collapsible.Content>
        {#if open}
          <div class="mt-2.5 space-y-3 border-t border-surface-200-800 pt-2.5">
            {#if info.length > 0}
              <dl class="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-[11px]">
                {#each info as line (line.label)}
                  <dt class="text-surface-500">{line.label}</dt>
                  <dd class="min-w-0 break-words text-surface-600-400">
                    {#if line.href}
                      <button
                        type="button"
                        onclick={() => commands.openExternalUrl(line.href ?? "")}
                        class="inline-flex items-center gap-1 text-primary-500 hover:underline"
                      >
                        {line.value} <ExternalLink size={10} />
                      </button>
                    {:else}
                      {line.value}
                    {/if}
                  </dd>
                {/each}
              </dl>
            {/if}
            {#if tools.length > 0}
              <div>
                <div class="mb-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">Tools</div>
                <ul class="space-y-2">
                  {#each tools as tool (tool.name)}
                    {@const parameters = toolParameters(tool)}
                    <li id="mcp-tool-{server.name}-{tool.name}">
                      <div class="font-mono text-[11px] font-medium">{tool.name}</div>
                      {#if tool.description}
                        <p class="mt-0.5 text-[11px] leading-snug text-surface-600-400">{tool.description}</p>
                      {/if}
                      {#if parameters.length > 0}
                        <dl class="mt-1 space-y-0.5">
                          {#each parameters as parameter (parameter.name)}
                            <div class="flex gap-1.5 text-[10px]">
                              <dt class="shrink-0 font-mono text-surface-600-400">
                                {parameter.name}{parameter.required ? "" : "?"}
                                <span class="text-surface-400">: {parameter.type}</span>
                              </dt>
                              {#if parameter.hint}
                                <dd class="min-w-0 flex-1 truncate text-surface-500">{parameter.hint}</dd>
                              {/if}
                            </div>
                          {/each}
                        </dl>
                      {/if}
                    </li>
                  {/each}
                </ul>
              </div>
            {/if}
            {#each uris as list (list.title)}
              {#if list.rows.length > 0}
                <div>
                  <div class="mb-1 text-[10px] font-semibold uppercase tracking-wide text-surface-500">
                    {list.title}
                  </div>
                  <ul class="space-y-1.5">
                    {#each list.rows as row (row.uri)}
                      <li>
                        <div class="flex items-baseline gap-2">
                          <span class="font-mono text-[11px] font-medium break-all">{row.uri}</span>
                          {#if row.mimeType}
                            <span class="shrink-0 text-[10px] text-surface-400">{row.mimeType}</span>
                          {/if}
                        </div>
                        {#if row.title || row.name || row.description}
                          <p class="mt-0.5 text-[11px] leading-snug text-surface-600-400">
                            {[row.title || row.name, row.description].filter(Boolean).join(" — ")}
                          </p>
                        {/if}
                      </li>
                    {/each}
                  </ul>
                </div>
              {/if}
            {/each}
          </div>
        {/if}
      </Collapsible.Content>
    {/if}
  </Collapsible>
</div>
