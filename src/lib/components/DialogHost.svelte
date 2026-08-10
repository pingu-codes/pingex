<script lang="ts">
import { closeDialog, dialogStack } from "$lib/app/dialogs.svelte";
</script>

<!-- Mounted once (App.svelte); renders whatever `openDialog` pushed. -->
{#each dialogStack as entry (entry.id)}
  {@const Dialog = entry.component}
  <Dialog {...entry.props} close={(result: unknown) => closeDialog(entry.id, result)} />
{/each}
