<script lang="ts">
  import { Bot, Check, ChevronDown } from "@lucide/svelte";
  import { Menu, Portal } from "@skeletonlabs/skeleton-svelte";
  import { HARNESS_INFO, HARNESS_KINDS } from "$lib/bindings";
  import type { HarnessChoice } from "$lib/composer/composerPrefs";
  import { availableHomes, chooseDefaultHome, currentHomeKey } from "$lib/services/homeRouting";
  import { harnessLabel } from "$lib/utils/capabilities";
  import { errorMessage } from "$lib/utils/errors";

  let { harness, onChoose }: { harness: HarnessChoice; onChoose: (next: HarnessChoice) => void } = $props();

  // Without registered Homes, offer the harnesses every Host has; an optional
  // one (maki) appears once a Home for it is added.
  const options: { value: HarnessChoice; label: string }[] = HARNESS_KINDS.filter(
    (kind) => !HARNESS_INFO[kind].optional,
  ).map((kind) => ({ value: kind, label: HARNESS_INFO[kind].label }));

  const label = $derived(harnessLabel(harness));
  const homes = $derived(availableHomes());
  let error = $state<string | null>(null);

  async function chooseHome(id: string, next: HarnessChoice) {
    try {
      await chooseDefaultHome(id);
      onChoose(next);
      error = null;
    } catch (cause) {
      error = errorMessage(cause);
    }
  }
</script>

<Menu positioning={{ placement: "top-end" }}>
  <Menu.Trigger aria-label="Choose harness" title="Harness new threads run on" class="pill">
    <Bot size={12} />
    {label}
    <ChevronDown size={11} />
  </Menu.Trigger>
  <Portal>
    <Menu.Positioner>
      <Menu.Content class="popover-card z-50 w-44 select-none p-1">
        {#if homes.length}
          {#each homes as entry (entry.home.id)}
            <Menu.OptionItem
              type="radio"
              value={entry.home.id}
              checked={harness === entry.home.harness && currentHomeKey(harness) === entry.homeKey}
              onCheckedChange={(checked) => {
                if (checked) void chooseHome(entry.home.id, entry.home.harness);
              }}
              class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs hover:preset-tonal"
            >
              <Menu.ItemText class="flex-1">{entry.home.label} · {harnessLabel(entry.home.harness)}</Menu.ItemText>
              <Menu.ItemIndicator class="hidden data-[state=checked]:block"
                ><Check size={13} class="text-primary-500" /></Menu.ItemIndicator
              >
            </Menu.OptionItem>
          {/each}
        {:else}
          {#each options as option (option.value)}
            <Menu.OptionItem
              type="radio"
              value={option.value}
              checked={harness === option.value}
              onCheckedChange={(checked) => checked && onChoose(option.value)}
              class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs hover:preset-tonal"
            >
              <Menu.ItemText class="flex-1">{option.label}</Menu.ItemText>
              <Menu.ItemIndicator class="hidden data-[state=checked]:block">
                <Check size={13} class="text-primary-500" />
              </Menu.ItemIndicator>
            </Menu.OptionItem>
          {/each}
        {/if}
        {#if error}
          <p role="alert" class="p-2 text-xs text-error-500">{error}</p>
        {/if}
      </Menu.Content>
    </Menu.Positioner>
  </Portal>
</Menu>
