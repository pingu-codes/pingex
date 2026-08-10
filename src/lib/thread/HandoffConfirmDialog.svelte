<script lang="ts">
import { Copy, TerminalSquare } from "@lucide/svelte";
import { type DialogClose, submitState } from "$lib/app/dialogs.svelte";
import DialogShell from "$lib/components/DialogShell.svelte";

let {
  home,
  threadId,
  dir,
  command,
  copy,
  launch,
  close,
}: {
  home: string;
  threadId: string;
  dir: string;
  /** The `codex resume` invocation this handoff produces. */
  command: string;
  copy: (command: string) => Promise<void>;
  launch: (command: string) => Promise<void>;
  close: DialogClose<true>;
} = $props();

let copied = $state(false);
const action = submitState();

async function copyCommand() {
  copied = await action.run(() => copy(command));
}

async function launchTerminal() {
  if (await action.run(() => launch(command))) close(true);
}
</script>

<DialogShell title="Continue in terminal" width={480} onClose={() => close()}>
  <p class="mt-2 text-sm leading-6 text-surface-600-400">
    This resumes the same thread in your terminal, using this exact Codex home, thread, and directory.
  </p>
  <dl class="mt-3 space-y-1.5 text-xs">
    <div class="flex gap-2">
      <dt class="w-16 shrink-0 text-surface-500">Home</dt>
      <dd class="min-w-0 flex-1 truncate font-mono text-surface-900-100" title={home}>{home}</dd>
    </div>
    <div class="flex gap-2">
      <dt class="w-16 shrink-0 text-surface-500">Thread</dt>
      <dd class="min-w-0 flex-1 truncate font-mono text-surface-900-100" title={threadId}>{threadId}</dd>
    </div>
    <div class="flex gap-2">
      <dt class="w-16 shrink-0 text-surface-500">Directory</dt>
      <dd class="min-w-0 flex-1 truncate font-mono text-surface-900-100" title={dir}>{dir}</dd>
    </div>
  </dl>
  <pre class="mt-3 overflow-x-auto rounded-lg border border-surface-200-800 bg-surface-100-900 px-3 py-2 text-[11px] leading-5 text-surface-800-200"><code>{command}</code></pre>

  {#if action.error}
    <div class="card preset-tonal-error mt-3 p-3 text-xs">{action.error}</div>
  {/if}

  {#snippet footer()}
    <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
    <button type="button" onclick={copyCommand} class="btn btn-sm preset-tonal-primary">
      <Copy size={13} />
      {copied ? "Copied" : "Copy command"}
    </button>
    <button type="button" onclick={launchTerminal} class="btn btn-sm preset-filled-primary-500">
      <TerminalSquare size={13} />
      Open Terminal
    </button>
  {/snippet}
</DialogShell>
