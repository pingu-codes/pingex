<script lang="ts">
  import { RotateCcw } from "@lucide/svelte";
  import TooltipButton from "$lib/components/TooltipButton.svelte";
  import SettingRow from "$lib/layout/settings/SettingRow.svelte";
  import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";
  import { DEFAULT_QUICK_SHORTCUT } from "$lib/services/api";
  import { commands } from "$lib/services/homeRouting";
  import { acceleratorFromEvent } from "$lib/utils/accelerator";
  import { errorMessage } from "$lib/utils/errors";

  const KEYBOARD_SHORTCUTS = [
    { keys: "↵", action: "Send message (while Codex works: steer it)" },
    { keys: "⇥", action: "While Codex works: queue for after the turn" },
    { keys: "⇧ ↵", action: "New line in composer" },
    { keys: "⌘ K", action: "Open file mention search" },
    { keys: "/", action: "Open slash command menu" },
    { keys: "Esc", action: "Interrupt the running turn" },
  ];

  // Quick-chat global shortcut recorder.
  let quickShortcut = $state<string>(DEFAULT_QUICK_SHORTCUT);
  let recording = $state(false);
  let shortcutError = $state<string | null>(null);

  $effect(() => {
    commands
      .getQuickShortcut()
      .then((accelerator) => (quickShortcut = accelerator))
      .catch(() => (quickShortcut = DEFAULT_QUICK_SHORTCUT));
  });

  async function applyShortcut(accelerator: string) {
    const previous = quickShortcut;
    shortcutError = null;
    try {
      quickShortcut = await commands.setQuickShortcut(accelerator);
    } catch (cause) {
      quickShortcut = previous;
      shortcutError = errorMessage(cause);
    }
  }

  function recordShortcut(event: KeyboardEvent) {
    if (!recording) return;
    event.preventDefault();
    // Escape never reaches here: the window handler below cancels first.
    const accelerator = acceleratorFromEvent(event);
    if (!accelerator) return; // still holding modifiers; wait for the final key
    recording = false;
    applyShortcut(accelerator);
  }

  async function resetShortcut() {
    recording = false;
    await applyShortcut(DEFAULT_QUICK_SHORTCUT);
  }
</script>

<!-- Escape cancels a recording before Settings sees it, wherever focus is. -->
<svelte:window
  onkeydowncapture={(event) => {
    if (!recording || event.key !== "Escape") return;
    event.preventDefault();
    recording = false;
  }}
/>

<SettingsCard title="Keyboard shortcuts" error={shortcutError}>
  <div class="mt-1">
    <SettingRow label="Quick chat" description="Open the floating composer from anywhere.">
      <button
        type="button"
        onclick={() => {
          recording = !recording;
          shortcutError = null;
        }}
        onkeydown={recordShortcut}
        class="input min-w-[150px] rounded-md px-3 py-1.5 text-center font-mono text-xs {recording
          ? "border-primary-500 text-primary-500"
          : ""}"
      >
        {recording ? "Press keys…" : quickShortcut}
      </button>
      <TooltipButton
        label="Reset to default"
        type="button"
        onclick={resetShortcut}
        aria-label="Reset quick chat shortcut to default"
        class="btn-icon btn-icon-sm hover:preset-tonal text-surface-500"
      >
        <RotateCcw size={14} />
      </TooltipButton>
    </SettingRow>
  </div>
  <div class="divide-y divide-surface-200-800 border-t border-surface-200-800 text-sm">
    {#each KEYBOARD_SHORTCUTS as shortcut (shortcut.action)}
      <div class="flex items-center justify-between py-2">
        <span>{shortcut.action}</span>
        <kbd class="kbd text-xs">{shortcut.keys}</kbd>
      </div>
    {/each}
  </div>
</SettingsCard>
