<!-- The logs of a PR's failing checks, as GitHub has them at its head. -->
<script lang="ts">
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import CheckLogList from "$lib/inbox/CheckLogList.svelte";
  import type { CheckLog } from "$lib/types";

  let { title, logs, close }: { title: string; logs: CheckLog[]; close: DialogClose<never> } = $props();
</script>

<DialogShell {title} width={720} onClose={() => close()}>
  <div class="mt-2 flex flex-col gap-2"><CheckLogList {logs} /></div>
</DialogShell>
