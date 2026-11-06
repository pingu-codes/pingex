<!--
  A refresh icon that spins while `spinning`. The SVG is rebuilt when it
  starts or stops: WebKit, toggling `animate-spin` on an icon already painted
  inside a filtered or faded button (a hovered, then disabled `.btn`), keeps
  the static paint and composites the spinning copy over it.
-->
<script lang="ts">
  import { RefreshCw } from "@lucide/svelte";

  let {
    spinning = false,
    size = 14,
    class: className = "",
  }: { spinning?: boolean; size?: number; class?: string } = $props();
</script>

{#key spinning}
  <RefreshCw {size} class="{spinning ? "animate-spin" : ""} {className}" aria-hidden="true" />
{/key}
