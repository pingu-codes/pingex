<!-- A GitHub account or team: its avatar, or round initials until the image
     loads or when it can't, with who it is on hover after 500ms. `role` says
     why they are on the row: a PR's author and its reviewers look alike
     otherwise. With `onclick` it is a toggle button, e.g. to filter the inbox
     by this person; `pressed` says whether it is on. -->
<script lang="ts">
  import { Avatar } from "@skeletonlabs/skeleton-svelte";
  import TooltipAnchor from "$lib/components/TooltipAnchor.svelte";
  import { initials, personLabel } from "$lib/inbox/inbox";
  import type { GitHubPerson } from "$lib/types";

  let {
    login,
    person = null,
    role = null,
    plain = false,
    onclick,
    pressed = false,
    avatarClass = "",
    class: className = "",
  }: {
    login: string;
    person?: GitHubPerson | null;
    role?: string | null;
    /** No hover card: for comments in a diff, where a machine per row costs scrolling. */
    plain?: boolean;
    onclick?: () => void;
    pressed?: boolean;
    /** On the avatar itself, e.g. a reviewer's ring. */
    avatarClass?: string;
    class?: string;
  } = $props();

  const label = $derived(personLabel(login, person, role));
  const hint = $derived(pressed ? "Click to stop filtering by them" : "Click to show only theirs");
</script>

{#snippet avatar()}
  <Avatar
    class="size-[18px] bg-surface-200-800 text-surface-700-300 {person?.isTeam ? "rounded-md" : ""} {avatarClass}"
  >
    {#if person?.avatarUrl}
      <Avatar.Image src={person.avatarUrl} alt="" />
    {/if}
    <Avatar.Fallback class="text-[9px] font-semibold">{initials(login)}</Avatar.Fallback>
  </Avatar>
{/snippet}

{#if plain}
  <span class="inline-flex shrink-0 {className}">{@render avatar()}</span>
{:else if onclick}
  <TooltipAnchor
    tag="button"
    label={`${label}\n${hint}`}
    multiline
    openDelay={500}
    aria-label={label}
    aria-pressed={pressed}
    class="inline-flex shrink-0 rounded-full outline-offset-1 focus-visible:outline-2 focus-visible:outline-primary-500 {pressed
      ? "ring-2 ring-primary-500"
      : ""} {className}"
    {onclick}
  >
    {@render avatar()}
  </TooltipAnchor>
{:else}
  <TooltipAnchor {label} openDelay={500} class="inline-flex shrink-0 {className}"> {@render avatar()} </TooltipAnchor>
{/if}
