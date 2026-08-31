<script lang="ts">
import { Check, ChevronLeft, ChevronRight, Copy, Pencil, Sparkles } from "@lucide/svelte";
import { convertFileSrc } from "@tauri-apps/api/core";
import { onDestroy, tick } from "svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { copyText, isTauri, revealInFinder } from "$lib/services/api";
import { messageText as joinMessageText, mergeTextParts, userMessageMarkdown } from "$lib/thread/messageText";
import type { MessageVersions } from "$lib/thread/messageVersions";
import { messageParts } from "$lib/thread/turnSegments";
import type { ThreadItem, UserInputPart } from "$lib/types";
import { fileIconFor, iconForPath } from "$lib/utils/fileIcons";
import { resolveMentionPath, splitMentions } from "$lib/utils/mentions";

let {
  item,
  editable,
  cwd = "",
  versions = null,
  onSubmitEdit,
  onSelectVersion,
}: {
  item: ThreadItem;
  editable: boolean;
  /** Thread cwd, used to resolve the relative paths Codex stores in mentions. */
  cwd?: string;
  /** Where this message sits among its edited versions; null when unedited. */
  versions?: MessageVersions | null;
  /** Called with the edited message — the original parts with the text
   *  replaced; starts a new version of the thread from here. */
  onSubmitEdit: (parts: UserInputPart[]) => void;
  /** Open the thread holding another version of this message. */
  onSelectVersion?: (threadId: string) => void;
} = $props();

let failedImages = $state<string[]>([]);
let draft = $state<string | null>(null);
let textarea = $state<HTMLTextAreaElement | null>(null);

// Mentions arrive as their own text parts; merge adjacent runs so a mention
// stays inline in the sentence it was typed into.
const parts = $derived(mergeTextParts(messageParts(item)));

const messageText = () => joinMessageText(messageParts(item));

let copied = $state(false);
let copyTimer: ReturnType<typeof setTimeout> | undefined;
onDestroy(() => clearTimeout(copyTimer));

function copyMessage() {
  copyText(userMessageMarkdown(messageParts(item), cwd)).catch(() => {});
  copied = true;
  clearTimeout(copyTimer);
  copyTimer = setTimeout(() => (copied = false), 1500);
}

async function startEdit() {
  draft = messageText();
  await tick();
  textarea?.focus();
  textarea?.setSelectionRange(textarea.value.length, textarea.value.length);
}

/** The original parts with their text replaced by `text`: attachments and
 *  skills keep their place, the merged text runs collapse into one. */
function editedParts(text: string): UserInputPart[] {
  const edited: UserInputPart[] = [];
  let placed = false;
  for (const part of parts) {
    if (part.type !== "text") {
      edited.push(part);
    } else if (!placed) {
      edited.push({ type: "text", text });
      placed = true;
    }
  }
  if (!placed) edited.push({ type: "text", text });
  return edited;
}

function submit() {
  const text = draft?.trim() ?? "";
  draft = null;
  if (text && text !== messageText()) onSubmitEdit(editedParts(text));
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    draft = null;
  } else if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
    event.preventDefault();
    submit();
  }
}

function autogrow() {
  const element = textarea;
  if (!element) return;
  element.style.height = "auto";
  element.style.height = `${element.scrollHeight}px`;
}

$effect(() => {
  void draft;
  autogrow();
});

const basename = (path: string) => path.split("/").pop() || path;

const localImageSrc = (part: UserInputPart) =>
  isTauri() && part.path && !failedImages.includes(part.path) ? convertFileSrc(part.path) : null;
</script>

{#snippet fileChip(name: string, path: string)}
  {@const glyph = iconForPath(name, path)}
  <button
    onclick={() => path && revealInFinder(path).catch(() => {})}
    title={path}
    class="inline-flex items-center gap-1.5 rounded-full bg-surface-200-800 px-2.5 py-1 align-baseline text-xs {path
      ? 'hover:preset-tonal'
      : 'cursor-default'}"
  >
    <glyph.icon size={12} class={glyph.class} />
    @{name}
  </button>
{/snippet}

<div class="group/bubble flex items-start justify-end gap-1.5">
  {#if draft === null}
    <TooltipButton
      label="Copy message"
      aria-label="Copy message"
      onclick={copyMessage}
      class="mt-2 grid size-6 shrink-0 place-items-center rounded text-surface-500 transition hover:bg-surface-200-800 hover:text-surface-800-200 {copied
        ? 'opacity-100'
        : 'opacity-0 group-hover/bubble:opacity-100'}"
    >
      {#if copied}
        <Check size={12} class="text-success-500" />
      {:else}
        <Copy size={12} />
      {/if}
    </TooltipButton>
  {/if}
  {#if editable && draft === null && messageParts(item).some((part) => part.type === "text")}
    <TooltipButton
      label="Edit message (starts a new version)"
      aria-label="Edit and resend"
      onclick={startEdit}
      class="mt-2 grid size-6 shrink-0 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-200-800 hover:text-surface-800-200 group-hover/bubble:opacity-100"
    >
      <Pencil size={12} />
    </TooltipButton>
  {/if}
  <div class="flex min-w-0 flex-col items-end gap-1 {draft === null ? 'max-w-[85%]' : 'w-full'}">
  <div
    class="min-w-0 space-y-2 rounded-2xl rounded-br-md bg-primary-500/10 px-4 py-2.5 text-sm leading-6 [overflow-wrap:anywhere] {draft === null
      ? ''
      : 'w-full'}"
  >
    {#each parts as part, index (index)}
      {#if part.type === "text" && part.text}
        {#if draft === null}
          <!-- Kept on one line: the container is `whitespace-pre-wrap`, so any
               indentation between the segments would render as real spaces. -->
          <div class="whitespace-pre-wrap"
            >{#each splitMentions(part.text) as segment, segmentIndex (segmentIndex)}{#if segment.type === "mention"}{@render fileChip(segment.name, resolveMentionPath(segment.path, cwd))}{:else}{segment.text}{/if}{/each}</div
          >
        {/if}
      {:else if part.type === "image" && part.url}
        <img src={part.url} alt="Attachment" class="max-h-64 max-w-full rounded-lg" />
      {:else if part.type === "localImage" && part.path}
        {#if localImageSrc(part) !== null}
          <img
            src={localImageSrc(part)}
            alt={basename(part.path)}
            class="max-h-64 max-w-full rounded-lg"
            onerror={() => part.path && (failedImages = [...failedImages, part.path])}
          />
        {:else}
          {@const glyph = fileIconFor(basename(part.path))}
          <button
            onclick={() => part.path && revealInFinder(part.path).catch(() => {})}
            title={part.path}
            class="inline-flex items-center gap-1.5 rounded-full bg-surface-200-800 px-2.5 py-1 text-xs hover:preset-tonal"
          >
            <glyph.icon size={12} class={glyph.class} />
            {basename(part.path)}
          </button>
        {/if}
      {:else if part.type === "skill" && part.name}
        <span class="inline-flex items-center gap-1.5 rounded-full bg-surface-200-800 px-2.5 py-1 text-xs">
          <Sparkles size={12} class="text-surface-500" />
          {part.name}
        </span>
      {:else if part.name || part.path}
        {@render fileChip(part.name ?? basename(part.path ?? ""), part.path ?? "")}
      {/if}
    {/each}
    {#if draft !== null}
      <textarea
        bind:this={textarea}
        bind:value={draft}
        onkeydown={onKeydown}
        aria-label="Edit message"
        rows="1"
        class="w-full resize-none bg-transparent outline-none"
      ></textarea>
      <div class="flex items-center justify-end gap-2">
        <button onclick={() => (draft = null)} class="btn btn-sm hover:preset-tonal">Cancel</button>
        <button onclick={submit} class="btn btn-sm preset-filled-primary-500" title="Starts a new version of the thread from this message">
          Send
        </button>
      </div>
    {/if}
  </div>
  {#if versions && draft === null}
    <div class="flex items-center gap-0.5 text-[11px] text-surface-500" aria-label="Message versions">
      <button
        aria-label="Previous version"
        disabled={!versions.prevThreadId}
        onclick={() => versions?.prevThreadId && onSelectVersion?.(versions.prevThreadId)}
        class="grid size-5 place-items-center rounded transition enabled:hover:bg-surface-200-800 enabled:hover:text-surface-800-200 disabled:opacity-40"
      >
        <ChevronLeft size={12} />
      </button>
      <span class="tabular-nums">{versions.index + 1} / {versions.count}</span>
      <button
        aria-label="Next version"
        disabled={!versions.nextThreadId}
        onclick={() => versions?.nextThreadId && onSelectVersion?.(versions.nextThreadId)}
        class="grid size-5 place-items-center rounded transition enabled:hover:bg-surface-200-800 enabled:hover:text-surface-800-200 disabled:opacity-40"
      >
        <ChevronRight size={12} />
      </button>
    </div>
  {/if}
  </div>
</div>
