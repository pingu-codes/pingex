<script lang="ts">
import { Check, MessageCircleQuestion, TriangleAlert } from "@lucide/svelte";
import { respondUserInput } from "$lib/services/api";
import { clearUnanswered, removeUserInputRequest, type UserInputRequest } from "$lib/services/codexEvents.svelte";
import type { ThreadItem } from "$lib/types";

let {
  request,
  onAnswered,
  onResume,
}: {
  request: UserInputRequest;
  onAnswered?: (item: ThreadItem) => void;
  /**
   * Supplied for a question stranded by an earlier session. There is no request
   * left to respond to, so the answer is sent on as a new message instead.
   */
  onResume?: (text: string) => Promise<void> | void;
} = $props();

const stranded = $derived(request.requestId === null);

let submitting = $state(false);
let selections = $state<Record<string, string | null>>({});
let notes = $state<Record<string, string>>({});
let steer = $state("");

const steering = $derived(steer.trim().length > 0);
const answerable = $derived(
  request.questions.every((question) => selections[question.id] || notes[question.id]?.trim()),
);
// Steering overrides the per-question requirement: the user can send a free-form
// message instead of answering any of the questions.
const submittable = $derived(steering || answerable);

function toggle(questionId: string, label: string) {
  selections[questionId] = selections[questionId] === label ? null : label;
}

async function submit() {
  if (submitting || !submittable) return;
  submitting = true;
  try {
    const steerText = steer.trim();
    const answers: Record<string, { answers: string[] }> = {};
    // Secret answers never leave the live request: they are masked before
    // persistence so the value only goes to Codex itself.
    const persistedAnswers: Record<string, { answers: string[] }> = {};
    for (const question of request.questions) {
      // When steering, none of the questions are answered — the steering
      // message stands in for every answer so Codex can't miss it.
      if (steerText) {
        answers[question.id] = { answers: [steerText] };
        continue;
      }
      const parts: string[] = [];
      const selected = selections[question.id];
      if (selected) parts.push(selected);
      const note = notes[question.id]?.trim();
      if (note) parts.push(selected ? `Note: ${note}` : note);
      answers[question.id] = { answers: parts };
      persistedAnswers[question.id] = question.isSecret ? { answers: ["••••"] } : { answers: parts };
    }
    const item: ThreadItem = {
      type: "userInputAnswered",
      id: request.itemId,
      questions: request.questions,
      answers: steerText ? {} : persistedAnswers,
      ...(steerText ? { steer: steerText } : {}),
    };
    // A stranded question has no live request: the answer only gets to Codex as
    // a new message, so send it first and record it once that succeeds.
    if (stranded) await onResume?.(steerText || resumeMessage(answers));
    await respondUserInput(request.requestId, answers, {
      threadId: request.threadId,
      turnId: request.turnId,
      itemId: request.itemId,
      item,
    });
    onAnswered?.(item);
    clearUnanswered(request.threadId);
    if (request.requestId !== null) removeUserInputRequest(request.requestId);
  } finally {
    submitting = false;
  }
}

/**
 * Restates the questions with their answers, since Codex has no memory of
 * asking. Uses the unmasked answers — this message is what reaches Codex.
 */
function resumeMessage(answers: Record<string, { answers: string[] }>): string {
  return request.questions
    .map((question) => `${question.question}\n${answers[question.id]?.answers.join(" · ") ?? ""}`)
    .join("\n\n");
}

/** Give up on a stranded question so it stops being flagged as needing an answer. */
async function dismiss() {
  if (submitting) return;
  submitting = true;
  try {
    const item: ThreadItem = {
      type: "userInputAnswered",
      id: request.itemId,
      questions: request.questions,
      answers: {},
      dismissed: true,
    };
    await respondUserInput(
      null,
      {},
      {
        threadId: request.threadId,
        turnId: request.turnId,
        itemId: request.itemId,
        item,
      },
    );
    onAnswered?.(item);
    clearUnanswered(request.threadId);
  } finally {
    submitting = false;
  }
}
</script>

<div class="card preset-tonal space-y-3 p-3 text-sm" class:border-warning-500={stranded}>
  <div class="flex items-center gap-2 text-xs font-semibold">
    {#if stranded}
      <TriangleAlert size={14} class="text-warning-500" />
      Codex asked this before the app closed
    {:else}
      <MessageCircleQuestion size={14} class="text-primary-500" />
      Codex has a question
    {/if}
  </div>
  {#if stranded}
    <p class="text-[11px] leading-4 text-surface-500">
      That turn ended with the session, so your answer goes back as a new message.
    </p>
  {/if}
  {#each request.questions as question (question.id)}
    <div class="space-y-2">
      {#if request.questions.length > 1 || question.header}
        <div class="text-[10px] font-semibold uppercase tracking-wide text-surface-500">{question.header}</div>
      {/if}
      <p class="text-xs leading-5">{question.question}</p>
      {#if question.options?.length}
        <div class="space-y-1">
          {#each question.options as option (option.label)}
            {@const selected = selections[question.id] === option.label}
            <button
              onclick={() => toggle(question.id, option.label)}
              disabled={submitting}
              class="flex w-full items-start gap-2 rounded-lg border px-2.5 py-1.5 text-left text-xs transition {selected
                ? 'border-primary-500 bg-primary-500/10'
                : 'border-surface-200-800 hover:preset-tonal'}"
            >
              <span class="grid size-4 shrink-0 place-items-center rounded-full border {selected ? 'border-primary-500 bg-primary-500 text-white' : 'border-surface-400-600'} mt-px">
                {#if selected}<Check size={10} />{/if}
              </span>
              <span class="min-w-0">
                <span class="font-medium">{option.label}</span>
                {#if option.description}
                  <span class="block text-[11px] leading-4 text-surface-500">{option.description}</span>
                {/if}
              </span>
            </button>
          {/each}
        </div>
      {/if}
      {#if question.isSecret}
        <input
          type="password"
          bind:value={notes[question.id]}
          disabled={submitting}
          placeholder="Enter value…"
          class="w-full rounded-lg border border-surface-200-800 bg-surface-50-950 px-2.5 py-1.5 text-xs outline-none focus:border-surface-400-600"
        />
      {:else}
        <textarea
          bind:value={notes[question.id]}
          disabled={submitting}
          onkeydown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault();
              submit();
            }
          }}
          rows="1"
          placeholder={question.options?.length ? "Add a note, or answer in your own words…" : "Answer…"}
          class="max-h-28 min-h-[1.75rem] w-full resize-none rounded-lg border border-surface-200-800 bg-surface-50-950 px-2.5 py-1.5 text-xs leading-5 outline-none placeholder:text-surface-500 focus:border-surface-400-600"
        ></textarea>
      {/if}
    </div>
  {/each}
  <div class="space-y-1.5 border-t border-surface-200-800 pt-2.5" class:opacity-60={steering}>
    <div class="text-[10px] font-semibold uppercase tracking-wide text-surface-500">
      Or steer instead
    </div>
    <textarea
      bind:value={steer}
      disabled={submitting}
      onkeydown={(event) => {
        if (event.key === "Enter" && !event.shiftKey) {
          event.preventDefault();
          submit();
        }
      }}
      rows="1"
      placeholder="Skip the questions and tell Codex what to do in your own words…"
      class="max-h-28 min-h-[1.75rem] w-full resize-none rounded-lg border border-surface-200-800 bg-surface-50-950 px-2.5 py-1.5 text-xs leading-5 outline-none placeholder:text-surface-500 focus:border-surface-400-600"
    ></textarea>
  </div>
  <div class="flex items-center justify-end gap-2 pt-0.5">
    {#if steering}
      <span class="mr-auto text-[11px] text-surface-500">Questions will be skipped.</span>
    {/if}
    {#if stranded}
      <button onclick={dismiss} disabled={submitting} class="btn btn-sm preset-tonal {steering ? '' : 'mr-auto'}">
        Dismiss
      </button>
    {/if}
    <button onclick={submit} disabled={submitting || !submittable} class="btn btn-sm preset-filled-primary-500">
      {#if stranded}
        Send as new message
      {:else}
        {steering ? "Steer instead" : "Send"}
      {/if}
    </button>
  </div>
</div>
