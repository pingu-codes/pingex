<script lang="ts">
  import Toggle from "$lib/components/Toggle.svelte";
  import {
    appearance,
    FONT_SIZE_MAX,
    FONT_SIZE_MIN,
    type FontChoice,
    type ThemeName,
  } from "$lib/layout/appearancePrefs.svelte";
  import SettingRow from "$lib/layout/settings/SettingRow.svelte";
  import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";
  import { sidebarPrefs } from "$lib/layout/sidebarPrefs.svelte";

  // The font-size slider previews its value while dragging and applies on release:
  // applying per input rescales the page (slider included) under the pointer.
  let draftFontSize = $state<number | null>(null);
</script>

<SettingsCard title="Appearance" description="Local to this app; not stored in Codex config.">
  <div class="mt-4 divide-y divide-surface-200-800">
    <SettingRow label="Theme" description="Colours and contrast. Follows your system light or dark mode." local>
      <select
        aria-label="Theme"
        class="select text-sm"
        value={appearance.prefs.theme}
        onchange={(event) => appearance.set({ theme: event.currentTarget.value as ThemeName })}
      >
        <option value="default">Default</option>
        <option value="readable">Readable (high contrast)</option>
      </select>
    </SettingRow>
    <SettingRow label="Font" description="Typeface for all text and code." local>
      <select
        aria-label="Font"
        class="select text-sm"
        value={appearance.prefs.font}
        onchange={(event) => appearance.set({ font: event.currentTarget.value as FontChoice })}
      >
        <option value="default">Default</option>
        <option value="hyperlegible">Atkinson Hyperlegible</option>
      </select>
    </SettingRow>
    <SettingRow label="Density" description="Spacing of lists and cards." local>
      <select
        aria-label="Density"
        class="select text-sm"
        value={appearance.prefs.density}
        onchange={(event) => appearance.set({ density: event.currentTarget.value as "comfortable" | "compact" })}
      >
        <option value="comfortable">Comfortable</option>
        <option value="compact">Compact</option>
      </select>
    </SettingRow>
    <SettingRow label="Font size" local>
      {#snippet description()}
        Base text size ({draftFontSize ?? appearance.prefs.fontSize}px). Applies when you let go.
      {/snippet}
      <input
        type="range"
        aria-label="Font size"
        min={FONT_SIZE_MIN}
        max={FONT_SIZE_MAX}
        value={draftFontSize ?? appearance.prefs.fontSize}
        oninput={(event) => (draftFontSize = Number(event.currentTarget.value))}
        onchange={(event) => {
          appearance.set({ fontSize: Number(event.currentTarget.value) });
          draftFontSize = null;
        }}
      >
    </SettingRow>
    <SettingRow
      label="Hide threads older than a day"
      description="Threads not touched in 24 hours fold behind “Show more”. Pinned and open threads stay visible."
      local
    >
      <Toggle
        checked={sidebarPrefs.hideOldThreads}
        label="Hide threads older than a day"
        testId="hide-old-threads-switch"
        onChange={(checked) => sidebarPrefs.setHideOldThreads(checked)}
      />
    </SettingRow>
  </div>
</SettingsCard>
