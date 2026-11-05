<script lang="ts" module>
  import type { Component } from "svelte";

  export type SegmentedOption<T extends string = string> = {
    value: T;
    label: string;
    /** Shown before the label, or instead of it with `iconOnly`. */
    icon?: Component<{ size?: number; class?: string }>;
    title?: string;
    disabled?: boolean;
  };
</script>

<script lang="ts" generics="T extends string">
  import { SegmentedControl } from "@skeletonlabs/skeleton-svelte";

  // One look for every "pick one of a few" control: a surface track with the
  // selected option raised. Skeleton renders it as a radio group, so each option
  // is a native radio input named by its label.
  let {
    value,
    options,
    onChange,
    label,
    size = "sm",
    fill = false,
    iconOnly = false,
    disabled = false,
    class: className = "",
  }: {
    value: T;
    options: readonly SegmentedOption<T>[];
    onChange: (value: T) => void;
    /** Accessible name of the group. */
    label: string;
    size?: "sm" | "md";
    /** Stretch the track to its container and share the width between options. */
    fill?: boolean;
    /** Render only each option's icon; the label stays as its accessible name and title. */
    iconOnly?: boolean;
    disabled?: boolean;
    /** Classes for the outer element, e.g. margins. */
    class?: string;
  } = $props();

  const itemSize = $derived(size === "md" ? "px-3 py-1.5 text-sm" : "px-2.5 py-1 text-xs");
</script>

<SegmentedControl
  {value}
  {disabled}
  orientation="horizontal"
  onValueChange={(details) => {
    if (details.value !== null && details.value !== value) onChange(details.value as T);
  }}
  class="gap-0 {fill ? "" : "items-start"} {className}"
>
  <SegmentedControl.Label class="sr-only">{label}</SegmentedControl.Label>
  <SegmentedControl.Control class="relative flex-wrap gap-0.5 rounded-lg border-0 bg-surface-200-800 p-0.5 ring-0">
    <SegmentedControl.Indicator class="rounded-md bg-surface-50-950 shadow-sm" />
    {#each options as option (option.value)}
      <SegmentedControl.Item
        value={option.value}
        disabled={option.disabled}
        title={option.title ?? (iconOnly ? option.label : undefined)}
        class="h-auto min-w-0 gap-1.5 rounded-md {itemSize} {fill ? "flex-1" : "flex-none"}"
      >
        <SegmentedControl.ItemText
          class="inline-flex items-center gap-1.5 text-surface-600-400 data-[state=checked]:font-medium data-[state=checked]:text-surface-950-50"
        >
          {#if option.icon}
            <option.icon size={size === "md" ? 14 : 12} />
          {/if}
          {#if iconOnly}
            <span class="sr-only">{option.label}</span>
          {:else}
            {option.label}
          {/if}
        </SegmentedControl.ItemText>
        <SegmentedControl.ItemHiddenInput />
      </SegmentedControl.Item>
    {/each}
  </SegmentedControl.Control>
</SegmentedControl>
