<!-- A GitHub login as round initials (Skeleton Avatar), with the login on
     hover after 500ms. -->
<script lang="ts">
  import { Avatar } from "@skeletonlabs/skeleton-svelte";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import { initials } from "$lib/inbox/inbox";

  let { login, class: className = "" }: { login: string; class?: string } = $props();
</script>

<TooltipAnchor label={login} openDelay={500} class="inline-flex shrink-0 {className}">
  <Avatar class="size-[18px] bg-surface-200-800 text-surface-700-300">
    <Avatar.Fallback class="text-[9px] font-semibold">{initials(login)}</Avatar.Fallback>
  </Avatar>
</TooltipAnchor>
