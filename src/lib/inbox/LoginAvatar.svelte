<!-- A GitHub account or team: its avatar, or round initials until the image
     loads or when it can't, with who it is on hover after 500ms. `role` says
     why they are on the row: a PR's author and its reviewers look alike
     otherwise. -->
<script lang="ts">
  import { Avatar } from "@skeletonlabs/skeleton-svelte";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import { initials, personLabel } from "$lib/inbox/inbox";
  import type { GitHubPerson } from "$lib/types";

  let {
    login,
    person = null,
    role = null,
    avatarClass = "",
    class: className = "",
  }: {
    login: string;
    person?: GitHubPerson | null;
    role?: string | null;
    /** On the avatar itself, e.g. a reviewer's ring. */
    avatarClass?: string;
    class?: string;
  } = $props();
</script>

<TooltipAnchor label={personLabel(login, person, role)} openDelay={500} class="inline-flex shrink-0 {className}">
  <Avatar
    class="size-[18px] bg-surface-200-800 text-surface-700-300 {person?.isTeam ? "rounded-md" : ""} {avatarClass}"
  >
    {#if person?.avatarUrl}
      <Avatar.Image src={person.avatarUrl} alt="" />
    {/if}
    <Avatar.Fallback class="text-[9px] font-semibold">{initials(login)}</Avatar.Fallback>
  </Avatar>
</TooltipAnchor>
