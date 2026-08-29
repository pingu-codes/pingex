<script lang="ts">
import { ShieldQuestion } from "@lucide/svelte";
import DiffBlock from "$lib/components/DiffBlock.svelte";
import { respondApproval, respondServerRequest } from "$lib/services/api";
import { type Approval, removeApproval } from "$lib/services/codexEvents.svelte";
import { permissionLines } from "$lib/thread/permissions";

let { approval }: { approval: Approval } = $props();

let submitting = $state(false);

const TITLES = {
  command: "Codex wants to run a command",
  fileChange: "Codex wants to edit files",
  permissions: "Codex wants extra access",
} as const;

/** A `writeStdin` approval (Codex ≥0.150) asks to feed a running command,
 *  not start one; the payload is the input, not a command line. */
const stdin = $derived(approval.kind === "command" && approval.approvalKind === "writeStdin");
const HARNESS_TITLES = {
  command: "Claude wants to run a command",
  fileChange: "Claude wants to edit files",
  permissions: "Claude wants to use a tool",
} as const;
const title = $derived(
  approval.harness === "claude"
    ? `${HARNESS_TITLES[approval.kind]}${approval.kind === "permissions" && approval.title ? `: ${approval.title}` : ""}`
    : stdin
      ? "Codex wants to send input to the running command"
      : TITLES[approval.kind],
);

/**
 * A permission request is answered with the profile being granted rather than a
 * decision word: granting echoes back what was asked for, declining sends an
 * empty profile. Everything else takes the plain `{decision}` shape.
 */
async function decide(decision: string) {
  if (submitting) return;
  submitting = true;
  try {
    if (approval.options) {
      // Another harness: the option id is the whole answer.
      await respondApproval(approval.requestId, decision);
    } else if (approval.kind === "permissions") {
      await respondServerRequest(approval.requestId, {
        permissions: decision === "decline" ? {} : (approval.permissions ?? {}),
        scope: decision === "acceptForSession" ? "session" : "turn",
      });
    } else {
      await respondApproval(approval.requestId, decision);
    }
  } finally {
    removeApproval(approval.requestId);
  }
}
</script>

<div class="card preset-tonal-warning space-y-2.5 p-3 text-sm">
  <div class="flex items-center gap-2 text-xs font-semibold">
    <ShieldQuestion size={14} />
    {title}
  </div>
  {#if approval.kind === "command"}
    {#if approval.command}
      <pre aria-label={stdin ? "input" : "command"} class="overflow-x-auto rounded-lg bg-surface-950/80 px-3 py-2 font-mono text-[11px] leading-5 text-surface-50">{approval.command}</pre>
    {/if}
    {#if approval.cwd}
      <div class="text-[11px] opacity-70">in {approval.cwd}</div>
    {/if}
  {:else if approval.kind === "permissions" && approval.options}
    {#if approval.description}
      <p class="text-xs opacity-80">{approval.description}</p>
    {/if}
  {:else if approval.kind === "permissions"}
    <ul class="space-y-1">
      {#each permissionLines(approval.permissions) as line (line)}
        <li class="font-mono text-[11px] leading-5">{line}</li>
      {/each}
    </ul>
    {#if approval.cwd}
      <div class="text-[11px] opacity-70">for {approval.cwd}</div>
    {/if}
  {:else}
    {#each approval.changes ?? [] as change (change.path)}
      <DiffBlock {change} />
    {/each}
  {/if}
  {#if approval.reason}
    <p class="text-xs opacity-80">{approval.reason}</p>
  {/if}
  <div class="flex flex-wrap gap-2 pt-0.5">
    {#if approval.options}
      {#each approval.options as option, index (option.optionId)}
        <button
          onclick={() => decide(option.optionId)}
          disabled={submitting}
          class="btn btn-sm {index === 0 && !approval.defaultToReject ? 'preset-filled-primary-500' : 'preset-tonal'}"
        >
          {option.name}
        </button>
      {/each}
    {:else}
      <button onclick={() => decide("accept")} disabled={submitting} class="btn btn-sm preset-filled-primary-500">Allow</button>
      <button onclick={() => decide("acceptForSession")} disabled={submitting} class="btn btn-sm preset-tonal">Allow for session</button>
      <button onclick={() => decide("decline")} disabled={submitting} class="btn btn-sm preset-tonal">Decline</button>
    {/if}
  </div>
</div>
