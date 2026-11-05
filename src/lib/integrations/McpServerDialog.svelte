<script lang="ts">
  import { Plug, Plus, X } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import Segmented, { type SegmentedOption } from "$lib/components/Segmented.svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import { envKeysValid, formatArgs, parseArgs } from "$lib/integrations/integrationsHelpers";
  import type { McpServerInput } from "$lib/services/api";
  import type { McpServerSummary } from "$lib/types";
  import { submitState } from "$lib/utils/actionState.svelte";

  let {
    server = null,
    submit,
    close,
  }: {
    /** The server to edit; null adds a new one. */
    server?: McpServerSummary | null;
    /** Saves the server; rejecting keeps the dialog open with the error. */
    submit: (input: McpServerInput) => Promise<void>;
    close: DialogClose<true>;
  } = $props();

  // Seeded once: the dialog is mounted per opening.
  // svelte-ignore state_referenced_locally
  const previousName = server?.name ?? null;
  // svelte-ignore state_referenced_locally
  let name = $state(server?.name ?? "");
  /** `"stdio"` (command + args + env) or `"http"` (url + bearer token env var).
   *  An entry with neither command nor url is malformed; editing it as stdio is
   *  the repair path, since that is the transport the command field writes. */
  // svelte-ignore state_referenced_locally
  let transport = $state<"stdio" | "http">(server?.transport === "http" ? "http" : "stdio");
  // svelte-ignore state_referenced_locally
  let command = $state(server?.command ?? "");
  // svelte-ignore state_referenced_locally
  let args = $state(formatArgs(server?.args ?? []));
  /** Existing secret values are never sent to the UI: show key names with empty,
   *  write-only value fields. Leaving them blank preserves the stored secrets. */
  // svelte-ignore state_referenced_locally
  let env = $state((server?.envKeys ?? []).map((key) => ({ key, value: "" })));
  // svelte-ignore state_referenced_locally
  let url = $state(server?.url ?? "");
  // svelte-ignore state_referenced_locally
  let bearerEnv = $state(server?.bearerTokenEnvVar ?? "");
  const action = submitState();

  const TRANSPORTS: SegmentedOption<"stdio" | "http">[] = [
    { value: "stdio", label: "stdio (command)" },
    { value: "http", label: "HTTP (url)" },
  ];

  /** The input to save, or why the form cannot be saved yet. */
  function validated(): McpServerInput | string {
    const trimmed = name.trim();
    if (!trimmed) return "Name is required.";
    if (transport === "http") {
      if (!url.trim()) return "URL is required.";
      return { previousName, name: trimmed, url: url.trim(), bearerTokenEnvVar: bearerEnv.trim() || null };
    }
    if (!command.trim()) return "Command is required.";
    const keys = env.map((row) => row.key.trim());
    if (!envKeysValid(keys)) return "Environment variable names must be unique and non-empty.";
    // Only send env pairs that have a value typed this session. Empty values keep
    // the existing secret (on edit) or are omitted (on add).
    const values: Record<string, string> = {};
    for (const row of env) {
      if (row.key.trim() && row.value) values[row.key.trim()] = row.value;
    }
    return {
      previousName,
      name: trimmed,
      command: command.trim(),
      args: parseArgs(args),
      env: values,
      // Rows still on screen define the desired key set; anything the user
      // deleted is pruned from the stored env by its absence.
      envKeys: keys.filter(Boolean),
    };
  }

  async function save(event: SubmitEvent) {
    event.preventDefault();
    const input = validated();
    if (typeof input === "string") {
      action.error = input;
      return;
    }
    if (await action.run(() => submit(input))) close(true);
  }
</script>

<DialogShell
  title={previousName === null ? "Add MCP server" : `Edit ${previousName}`}
  width={480}
  onClose={() => close()}
>
  {#snippet icon()}
    <Plug size={17} class="text-primary-500" />
  {/snippet}

  <form onsubmit={save} class="mt-4 space-y-3">
    <div>
      <label for="mcp-name" class="text-xs font-medium text-surface-500">Name</label>
      <input
        id="mcp-name"
        bind:value={name}
        placeholder="github"
        autocomplete="off"
        class="input mt-1 w-full font-mono text-xs"
      >
      {#if previousName !== null && name.trim() !== previousName}
        <p class="mt-1 text-[11px] text-surface-500">Renaming keeps this server's settings and stored secrets.</p>
      {/if}
    </div>
    <div>
      <span class="text-xs font-medium text-surface-500">Transport</span>
      <Segmented
        label="Transport"
        value={transport}
        onChange={(next) => (transport = next)}
        class="mt-1"
        options={TRANSPORTS}
      />
    </div>
    {#if transport === "stdio"}
      <div>
        <label for="mcp-command" class="text-xs font-medium text-surface-500">Command</label>
        <input
          id="mcp-command"
          bind:value={command}
          placeholder="npx"
          autocomplete="off"
          class="input mt-1 w-full font-mono text-xs"
        >
      </div>
      <div>
        <label for="mcp-args" class="text-xs font-medium text-surface-500">Arguments</label>
        <input
          id="mcp-args"
          bind:value={args}
          placeholder="-y @modelcontextprotocol/server-github"
          autocomplete="off"
          class="input mt-1 w-full font-mono text-xs"
        >
      </div>
      <div>
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-surface-500">Environment variables</span>
          <button
            type="button"
            onclick={() => env.push({ key: "", value: "" })}
            class="btn btn-sm hover:preset-tonal text-xs text-surface-500"
          >
            <Plus size={12} />
            Add
          </button>
        </div>
        {#if previousName !== null && env.length > 0}
          <p class="mt-1 text-[11px] text-surface-500">
            Leave a value blank to keep the stored secret. Secret values are write-only.
          </p>
        {/if}
        <div class="mt-1.5 space-y-1.5">
          {#each env as row, index (index)}
            <div class="flex items-center gap-1.5">
              <input
                bind:value={row.key}
                placeholder="GITHUB_TOKEN"
                autocomplete="off"
                class="input flex-1 font-mono text-xs"
              >
              <input
                bind:value={row.value}
                type="password"
                placeholder={previousName !== null ? "unchanged" : "secret value"}
                autocomplete="off"
                class="input flex-1 font-mono text-xs"
              >
              <TooltipButton
                label="Remove variable"
                type="button"
                onclick={() => env.splice(index, 1)}
                aria-label="Remove variable"
                class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
              >
                <X size={14} />
              </TooltipButton>
            </div>
          {/each}
        </div>
      </div>
    {:else}
      <div>
        <label for="mcp-url" class="text-xs font-medium text-surface-500">URL</label>
        <input
          id="mcp-url"
          bind:value={url}
          placeholder="https://mcp.example.com/sse"
          autocomplete="off"
          class="input mt-1 w-full font-mono text-xs"
        >
      </div>
      <div>
        <label for="mcp-bearer" class="text-xs font-medium text-surface-500">Bearer token variable</label>
        <input
          id="mcp-bearer"
          bind:value={bearerEnv}
          placeholder="LINEAR_API_KEY"
          autocomplete="off"
          class="input mt-1 w-full font-mono text-xs"
        >
        <p class="mt-1 text-[11px] text-surface-500">
          Name of an environment variable holding the token. Leave blank to sign in with OAuth instead.
        </p>
      </div>
    {/if}
    {#if action.error}
      <div class="error-note">{action.error}</div>
    {/if}
    <div class="flex justify-end gap-2 pt-1">
      <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
      <button type="submit" disabled={action.busy} class="btn btn-sm preset-filled-primary-500">
        {action.busy ? "Saving…" : previousName === null ? "Add server" : "Save changes"}
      </button>
    </div>
  </form>
</DialogShell>
