<script lang="ts">
  import Toggle from "$lib/components/Toggle.svelte";
  import { messageLog } from "$lib/layout/messageLogPrefs.svelte";
  import SettingRow from "$lib/layout/settings/SettingRow.svelte";
  import SettingsCard from "$lib/layout/settings/SettingsCard.svelte";

  // Opening Advanced pulls the buffered traffic captured before this view
  // mounted; live messages arrive on the event stream from there.
  $effect(() => {
    if (messageLog.enabled) void messageLog.refresh();
  });
</script>

<SettingsCard title="Advanced" description="Local to this app; not stored in Codex config.">
  <div class="mt-1">
    <SettingRow
      label="Message log"
      description="Record the messages exchanged with the agent. Off by default; the log is kept in memory only and is discarded when you switch it off or quit. Once enabled, open it from a thread's overview menu to view it alongside the conversation."
      local
    >
      <Toggle
        checked={messageLog.enabled}
        label="Message log"
        testId="message-log-switch"
        onChange={(checked) => messageLog.setEnabled(checked)}
      />
    </SettingRow>
  </div>
</SettingsCard>
