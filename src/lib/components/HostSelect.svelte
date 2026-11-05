<script lang="ts">
  import type { HTMLSelectAttributes } from "svelte/elements";
  import { launch } from "$lib/app/launch.svelte";

  // The native/WSL host picker. Its value is a `hostToOption` string: "native"
  // or a distribution name. The distributions come from launch state, read once
  // at startup; a selected distribution missing from that list is still offered.
  let {
    value = $bindable("native"),
    distros,
    class: className = "",
    ...rest
  }: Omit<HTMLSelectAttributes, "value"> & {
    value?: string;
    /** Overrides the distributions from launch state. */
    distros?: string[];
  } = $props();

  const available = $derived(distros ?? launch.distros);
  const listed = $derived(
    value === "native" || !value || available.includes(value) ? available : [...available, value],
  );
</script>

<select bind:value class="select {className}" {...rest}>
  <option value="native">This computer</option>
  {#each listed as distro (distro)}
    <option value={distro}>WSL · {distro}</option>
  {/each}
</select>
