<script lang="ts">
  import { Sparkles } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import { validSkillName } from "$lib/integrations/integrationsHelpers";
  import type { NewSkillInput } from "$lib/services/api";
  import { submitState } from "$lib/utils/actionState.svelte";

  let {
    submit,
    close,
  }: {
    /** Creates the skill; rejecting keeps the dialog open with the error. */
    submit: (input: NewSkillInput) => Promise<void>;
    /** Resolves the new skill's name. */
    close: DialogClose<string>;
  } = $props();

  let name = $state("");
  let description = $state("");
  let body = $state("");
  const action = submitState();

  async function create(event: SubmitEvent) {
    event.preventDefault();
    const trimmed = name.trim();
    if (!validSkillName(trimmed)) {
      action.error = "Name must be lowercase letters, digits, '-' or '_' (not leading).";
      return;
    }
    if (!description.trim()) {
      action.error = "Description is required — it is what Codex matches against.";
      return;
    }
    const input = { name: trimmed, description: description.trim(), body: body.trim() || null };
    if (await action.run(() => submit(input))) close(trimmed);
  }
</script>

<DialogShell title="Add skill" width={480} onClose={() => close()}>
  {#snippet icon()}
    <Sparkles size={17} class="text-tertiary-500" />
  {/snippet}

  <form onsubmit={create} class="mt-4 space-y-3">
    <div>
      <label for="skill-name" class="text-xs font-medium text-surface-500">Name</label>
      <input
        id="skill-name"
        bind:value={name}
        placeholder="release-notes"
        autocomplete="off"
        class="input mt-1 w-full font-mono text-xs"
      >
      <p class="mt-1 text-[11px] text-surface-500">
        Becomes the folder name under <span class="font-mono">~/.codex/skills/</span>.
      </p>
    </div>
    <div>
      <label for="skill-description" class="text-xs font-medium text-surface-500">Description</label>
      <input
        id="skill-description"
        bind:value={description}
        placeholder="Write release notes from merged PRs. Use when asked to draft a changelog."
        class="input mt-1 w-full text-xs"
      >
      <p class="mt-1 text-[11px] text-surface-500">
        Codex decides when to use the skill from this sentence, so say what it does and when.
      </p>
    </div>
    <div>
      <label for="skill-body" class="text-xs font-medium text-surface-500">Instructions (optional)</label>
      <textarea
        id="skill-body"
        bind:value={body}
        rows={5}
        placeholder="## Instructions&#10;&#10;1. ..."
        class="textarea mt-1 w-full font-mono text-xs"
      ></textarea>
    </div>
    {#if action.error}
      <div class="error-note">{action.error}</div>
    {/if}
    <div class="flex justify-end gap-2 pt-1">
      <button type="button" onclick={() => close()} class="btn btn-sm preset-tonal">Cancel</button>
      <button type="submit" disabled={action.busy} class="btn btn-sm preset-filled-primary-500">
        {action.busy ? "Creating…" : "Create skill"}
      </button>
    </div>
  </form>
</DialogShell>
