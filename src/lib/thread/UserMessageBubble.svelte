<script lang="ts">
import { Check, ChevronLeft, ChevronRight, Copy, Pencil, Sparkles } from "@lucide/svelte";
import { convertFileSrc } from "@tauri-apps/api/core";
import { onDestroy } from "svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { isTauri, revealInFinder } from "$lib/services/api";
import { InlineEdit } from "$lib/thread/inlineEdit.svelte";
import {
  editedParts,
  messageText as joinMessageText,
  mergeTextParts,
  userMessageMarkdown,
} from "$lib/thread/messageText";
import type { MessageVersions } from "$lib/thread/messageVersions";
import { messageParts } from "$lib/thread/turnSegments";
import type { ThreadItem, UserInputPart } from "$lib/types";
import { copyPlainOrMarkdown } from "$lib/utils/copy";
import { fileIconFor, iconForPath } from "$lib/utils/fileIcons";
import { flashFlag } from "$lib/utils/flash.svelte";
import { resolveMentionPath, splitMentions } from "$lib/utils/mentions";
import { basename } from "$lib/utils/path";

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

// Mentions arrive as their own text parts; merge adjacent runs so a mention
// stays inline in the sentence it was typed into.
const parts = $derived(mergeTextParts(messageParts(item)));

const messageText = () => joinMessageText(messageParts(item));

const copied = flashFlag();
onDestroy(copied.clear);

function copyMessage(event: MouseEvent) {
  copyPlainOrMarkdown(event, messageText(), userMessageMarkdown(messageParts(item), cwd));
  copied.trigger();
}

const edit = new InlineEdit(messageText, (text) => onSubmitEdit(editedParts(parts, text)));

function autogrow() {
  const element = edit.textarea;
  if (!element) return;
  element.style.height = "auto";
  element.style.height = `${element.scrollHeight}px`;
}

$effect(() => {
  void edit.draft;
  autogrow();
});

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
  {#if !edit.editing}
    <TooltipButton
      label="Copy message (⇧ for Markdown)"
      aria-label="Copy message"
      onclick={copyMessage}
      class="mt-2 grid size-6 shrink-0 place-items-center rounded text-surface-500 transition hover:bg-surface-200-800 hover:text-surface-800-200 {copied.on
        ? 'opacity-100'
        : 'opacity-0 group-hover/bubble:opacity-100'}"
    >
      {#if copied.on}
        <Check size={12} class="text-success-500" />
      {:else}
        <Copy size={12} />
      {/if}
    </TooltipButton>
  {/if}
  {#if editable && !edit.editing && messageParts(item).some((part) => part.type === "text")}
    <TooltipButton
      label="Edit message (starts a new version)"
      aria-label="Edit and resend"
      onclick={edit.start}
      class="mt-2 grid size-6 shrink-0 place-items-center rounded text-surface-500 opacity-0 transition hover:bg-surface-200-800 hover:text-surface-800-200 group-hover/bubble:opacity-100"
    >
      <Pencil size={12} />
    </TooltipButton>
  {/if}
  <div class="flex min-w-0 flex-col items-end gap-1 {!edit.editing ? 'max-w-[85%]' : 'w-full'}">
  <div
    class="min-w-0 space-y-2 rounded-2xl rounded-br-md bg-primary-500/10 px-4 py-2.5 text-sm leading-6 [overflow-wrap:anywhere] {!edit.editing
      ? ''
      : 'w-full'}"
  >
    {#each parts as part, index (index)}
      {#if part.type === "text" && part.text}
        {#if !edit.editing}
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
    {#if edit.editing}
      <textarea
        bind:this={edit.textarea}
        bind:value={edit.draft}
        onkeydown={edit.onkeydown}
        aria-label="Edit message"
        rows="1"
        class="w-full resize-none bg-transparent outline-none"
      ></textarea>
      <div class="flex items-center justify-end gap-2">
        <button onclick={edit.cancel} class="btn btn-sm hover:preset-tonal">Cancel</button>
        <button onclick={edit.submit} class="btn btn-sm preset-filled-primary-500" title="Starts a new version of the thread from this message">
          Send
        </button>
      </div>
    {/if}
  </div>
  {#if versions && !edit.editing}
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
