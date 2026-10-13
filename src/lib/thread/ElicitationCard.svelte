<script lang="ts">
import { Check, ExternalLink, Plug } from "@lucide/svelte";
import { untrack } from "svelte";
import { respondServerRequest } from "$lib/services/api";
import { type Elicitation, removeElicitation } from "$lib/services/codexEvents.svelte";
import { buildContent, formFields, initialValues, isComplete } from "$lib/thread/elicitationForm";
import { submitState } from "$lib/utils/actionState.svelte";
import { errorMessage } from "$lib/utils/errors";

let { elicitation }: { elicitation: Elicitation } = $props();

const reply = submitState({ describe: (cause) => `Could not reply: ${errorMessage(cause)}` });

// `form` is the only mode carrying a schema this app can draw controls for.
// `openai/form` sends an opaque schema and `url` sends the user to a web page,
// so both are shown as the message plus a plain accept/decline.
const fields = $derived(elicitation.mode === "form" ? formFields(elicitation.requestedSchema) : []);
// Seeded once: a card is keyed by its request, whose schema never changes.
let values = $state<Record<string, unknown>>(
  untrack(() => initialValues(elicitation.mode === "form" ? formFields(elicitation.requestedSchema) : [])),
);
const submittable = $derived(isComplete(fields, values));

function toggleOption(name: string, value: string) {
  const current = Array.isArray(values[name]) ? (values[name] as string[]) : [];
  values[name] = current.includes(value) ? current.filter((entry) => entry !== value) : [...current, value];
}

async function respond(action: "accept" | "decline" | "cancel") {
  await reply.run(async () => {
    await respondServerRequest(elicitation.requestId, {
      action,
      content: action === "accept" ? buildContent(fields, values) : null,
      _meta: elicitation.meta ?? null,
    });
    // Only once answered: the MCP request is still waiting if this failed,
    // and removing the card left the turn hanging with nothing to retry.
    removeElicitation(elicitation.requestId);
  });
}
</script>

<div class="card preset-tonal space-y-3 p-3 text-sm">
  <div class="flex items-center gap-2 text-xs font-semibold">
    <Plug size={14} class="text-primary-500" />
    {elicitation.serverName || "An MCP server"} needs some input
  </div>
  {#if elicitation.message}
    <p class="text-xs leading-5">{elicitation.message}</p>
  {/if}

  {#if elicitation.mode === "url" && elicitation.url}
    <a
      href={elicitation.url}
      target="_blank"
      rel="noreferrer"
      class="inline-flex items-center gap-1.5 text-xs text-primary-500 hover:underline"
    >
      <ExternalLink size={12} />
      {elicitation.url}
    </a>
    <p class="text-[11px] leading-4 text-surface-500">
      Finish there, then confirm below so Codex can carry on.
    </p>
  {:else if elicitation.mode !== "form"}
    <p class="text-[11px] leading-4 text-surface-500">
      This server asked for a form this app can't draw. Decline unless you know what it wants.
    </p>
  {/if}

  {#each fields as field (field.name)}
    <div class="space-y-1.5">
      <div class="text-[10px] font-semibold uppercase tracking-wide text-surface-500">
        {field.label}{field.required ? " *" : ""}
      </div>
      {#if field.description}
        <p class="text-[11px] leading-4 text-surface-500">{field.description}</p>
      {/if}

      {#if field.control === "boolean"}
        <label class="flex items-center gap-2 text-xs">
          <input type="checkbox" bind:checked={values[field.name] as boolean} disabled={reply.busy} class="checkbox" />
          Yes
        </label>
      {:else if field.control === "select" || field.control === "multiselect"}
        <div class="space-y-1">
          {#each field.options as option (option.const)}
            {@const selected =
              field.control === "multiselect"
                ? ((values[field.name] as string[]) ?? []).includes(option.const)
                : values[field.name] === option.const}
            <button
              onclick={() =>
                field.control === "multiselect"
                  ? toggleOption(field.name, option.const)
                  : (values[field.name] = selected ? null : option.const)}
              disabled={reply.busy}
              class="flex w-full items-center gap-2 rounded-lg border px-2.5 py-1.5 text-left text-xs transition {selected
                ? 'border-primary-500 bg-primary-500/10'
                : 'border-surface-200-800 hover:preset-tonal'}"
            >
              <span
                class="grid size-4 shrink-0 place-items-center border {field.control === 'multiselect'
                  ? 'rounded'
                  : 'rounded-full'} {selected ? 'border-primary-500 bg-primary-500 text-white' : 'border-surface-400-600'}"
              >
                {#if selected}<Check size={10} />{/if}
              </span>
              {option.title}
            </button>
          {/each}
        </div>
      {:else}
        <input
          type={field.control === "number" ? "number" : field.format === "email" ? "email" : "text"}
          bind:value={values[field.name] as string}
          disabled={reply.busy}
          placeholder={String(field.default ?? "")}
          class="w-full rounded-lg border border-surface-200-800 bg-surface-50-950 px-2.5 py-1.5 text-xs outline-none focus:border-surface-400-600"
        />
      {/if}
    </div>
  {/each}

  <div class="flex items-center justify-end gap-2 pt-0.5">
    <button onclick={() => respond("cancel")} disabled={reply.busy} class="btn btn-sm preset-tonal mr-auto">
      Cancel
    </button>
    <button onclick={() => respond("decline")} disabled={reply.busy} class="btn btn-sm preset-tonal">Decline</button>
    <button
      onclick={() => respond("accept")}
      disabled={reply.busy || !submittable}
      class="btn btn-sm preset-filled-primary-500"
    >
      {fields.length ? "Send" : "Continue"}
    </button>
  </div>
  {#if reply.error}
    <p class="text-[11px] text-error-500" role="alert">{reply.error}</p>
  {/if}
</div>
