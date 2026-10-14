<script lang="ts">
import type { Snippet } from "svelte";
import type { HTMLAttributes } from "svelte/elements";
import SettingBadge from "$lib/layout/settings/SettingBadge.svelte";

// A label and description on the left, the control on the right. Rows sit in
// a `divide-y` list inside a SettingsCard.
let {
  label,
  description,
  local = false,
  badges,
  children,
  ...rest
}: {
  label: string;
  /** A snippet when the description needs markup or reactive text. */
  description?: string | Snippet;
  /** Mark the setting as stored by this app only, not in the harness config. */
  local?: boolean;
  /** Chips beside the label. */
  badges?: Snippet;
  children: Snippet;
} & HTMLAttributes<HTMLDivElement> = $props();
</script>

<div class="flex items-center justify-between gap-4 py-3" {...rest}>
  <div class="min-w-0">
    <div class="flex items-center gap-2">
      <span class="text-sm font-medium">{label}</span>
      {@render badges?.()}
    </div>
    {#if description}
      <div class="mt-0.5 text-[11px] leading-4 text-surface-500">
        {#if typeof description === "string"}{description}{:else}{@render description()}{/if}
      </div>
    {/if}
  </div>
  <div class="flex shrink-0 items-center gap-2">
    {#if local}<SettingBadge kind="local" />{/if}
    {@render children()}
  </div>
</div>
