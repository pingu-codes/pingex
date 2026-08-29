<script lang="ts">
import { KeyRound } from "@lucide/svelte";
import { authRecovery } from "$lib/services/codexEvents.svelte";

let { threadId }: { threadId: string | null } = $props();

/** Set while Codex re-authenticates with the provider mid-turn (unstable
 *  Codex); the turn carries on by itself once it is done. */
const recovery = $derived(threadId ? (authRecovery.byThread[threadId] ?? null) : null);
</script>

{#if recovery}
  <span
    role="status"
    title={recovery.message ?? undefined}
    class="flex items-center gap-1 rounded-full bg-warning-500/15 px-2 py-0.5 text-[11px] text-warning-700-300"
  >
    <KeyRound size={12} class="animate-pulse" />
    Re-authenticating{recovery.provider ? ` with ${recovery.provider}` : ""}…
  </span>
{/if}
