<script lang="ts">
// The small chip beside a setting that says where its value lives or what a
// change costs.
let { kind }: { kind: "restart" | "local" | "config" | "default" } = $props();
</script>

{#if kind === "restart"}
  <span class="chip preset-tonal-warning text-[10px]">Restart required</span>
{:else if kind === "local"}
  <span class="chip preset-tonal text-[10px] text-surface-500">Local</span>
{:else if kind === "config"}
  <span class="chip preset-tonal-primary text-[10px]">config.toml</span>
{:else}
  <span class="chip preset-tonal text-[10px] text-surface-500">Default</span>
{/if}
