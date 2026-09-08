<script lang="ts">
import { Bot, Check, ChevronDown } from "@lucide/svelte";
import { Menu, Portal } from "@skeletonlabs/skeleton-svelte";
import type { HarnessChoice } from "$lib/composer/composerPrefs.svelte";
import { availableHomes, chooseDefaultHome, currentHomeKey } from "$lib/services/homeRouting";

let { harness, onChoose }: { harness: HarnessChoice; onChoose: (next: HarnessChoice) => void } = $props();

const options: { value: HarnessChoice; label: string }[] = [
  { value: "codex", label: "Codex" },
  { value: "claude", label: "Claude Code" },
];

const label = $derived(options.find((option) => option.value === harness)?.label ?? harness);
const homes = $derived(availableHomes());
let error = $state<string | null>(null);

async function chooseHome(id: string, next: HarnessChoice) {
  try { await chooseDefaultHome(id); onChoose(next); error = null; }
  catch (cause) { error = cause instanceof Error ? cause.message : String(cause); }
}
</script>

<Menu positioning={{ placement: "top-end" }}>
  <Menu.Trigger
    aria-label="Choose harness"
    title="Harness new threads run on"
    class="inline-flex items-center gap-1.5 rounded-full px-2 py-1 text-[11px] text-surface-500 transition hover:bg-surface-200-800 hover:text-surface-800-200"
  >
    <Bot size={12} />
    {label}
    <ChevronDown size={11} />
  </Menu.Trigger>
  <Portal>
    <Menu.Positioner>
      <Menu.Content
        class="card z-50 w-44 select-none border border-surface-200-800 bg-surface-50-950 p-1 shadow-xl"
      >
        {#if homes.length}
          {#each homes as entry (entry.home.id)}
            <Menu.OptionItem type="radio" value={entry.home.id}
              checked={harness === entry.home.harness && currentHomeKey(harness) === entry.homeKey}
              onCheckedChange={(checked) => { if (checked) void chooseHome(entry.home.id, entry.home.harness); }}
              class="flex w-full cursor-pointer items-center gap-2 rounded px-2 py-1.5 text-xs hover:preset-tonal">
              <Menu.ItemText class="flex-1">{entry.home.label} · {entry.home.harness === "claude" ? "Claude Code" : "Codex"}</Menu.ItemText>
              <Menu.ItemIndicator class="hidden data-[state=checked]:block"><Check size={13} class="text-primary-500" /></Menu.ItemIndicator>
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
        {#if error}<p role="alert" class="p-2 text-xs text-error-500">{error}</p>{/if}
      </Menu.Content>
    </Menu.Positioner>
  </Portal>
</Menu>
