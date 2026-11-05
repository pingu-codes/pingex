<script lang="ts">
  /**
   * A read-only look at a file an agent cited, so following a `path:line` link
   * does not mean leaving the app.
   *
   * The dialog is a pure renderer: `openFilePreview` has already done the read,
   * so there is no loading flicker and the failure states are just props. It
   * opens whether or not the file could be read — a binary or a deleted path
   * still needs to say so, and still needs Finder and Zed one click away.
   */
  import { Check, Copy, FolderOpen, SquareArrowOutUpRight } from "@lucide/svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import { commands } from "$lib/services/homeRouting";
  import type { FilePreview } from "$lib/types";
  import { languageForPath } from "$lib/utils/codeLanguage";
  import { fileIconFor } from "$lib/utils/fileIcons";
  import { flashKey } from "$lib/utils/flash.svelte";
  import { formatBytes } from "$lib/utils/format";
  import { relativeMentionPath } from "$lib/utils/mentions";
  import { basename } from "$lib/utils/path";
  import CodeView from "./CodeView.svelte";

  let {
    path,
    line = null,
    column = null,
    cwd = null,
    preview,
    close,
  }: {
    /** The absolute host path, already stripped of any `:line` suffix. */
    path: string;
    line?: number | null;
    column?: number | null;
    /** Project root, for the relative form of the path. */
    cwd?: string | null;
    preview: FilePreview;
    close: DialogClose<void>;
  } = $props();

  const name = $derived(basename(path));
  const fileIcon = $derived(fileIconFor(name));
  const relative = $derived(cwd ? relativeMentionPath(path, cwd) : path);
  /** Hide the second copy button when it would put the same text on the clipboard. */
  const hasRelative = $derived(relative !== path);
  const cited = $derived(line === null ? "" : column === null ? `:${line}` : `:${line}:${column}`);

  const language = $derived(languageForPath(path));
  const lineCount = $derived(preview.text ? preview.text.split("\n").length : 0);

  const REASONS: Record<string, string> = {
    binary: "This looks like a binary file, so there is nothing to show.",
    missing: "This file no longer exists.",
    denied: "This file could not be opened — permission denied.",
    directory: "This path is a folder, not a file.",
  };
  const reason = $derived(
    preview.problem
      ? (REASONS[preview.problem] ?? `This file could not be read${preview.detail ? `: ${preview.detail}` : "."}`)
      : null,
  );

  /** Shown alongside the code rather than instead of it. */
  const notes = $derived.by(() => {
    const list: string[] = [];
    if (preview.truncated)
      list.push(`Showing the first ${formatBytes(preview.text.length)} of ${formatBytes(preview.bytes)}.`);
    if (line !== null && lineCount > 0 && line > lineCount) list.push(`Line ${line} is past the previewed region.`);
    return list;
  });

  const copied = flashKey<"absolute" | "relative">(1200);

  async function copy(which: "absolute" | "relative") {
    await commands.copyText(which === "absolute" ? path : relative);
    copied.trigger(which);
  }
</script>

<DialogShell title={name} subtitle="{relative}{cited}" width={1000} onClose={() => close()}>
  {#snippet icon()}
    {@const Glyph = fileIcon.icon}
    <Glyph size={15} class={fileIcon.class} />
  {/snippet}
  <div class="mt-4">
    {#if reason}
      <div class="panel-card px-4 py-6 text-center text-xs text-surface-500" data-testid="preview-problem">
        {reason}
      </div>
    {:else}
      <CodeView text={preview.text} {language} highlightLine={line} />
    {/if}
    {#each notes as note (note)}
      <p class="mt-2 text-[11px] text-surface-500">{note}</p>
    {/each}
  </div>

  {#snippet footer()}
    <button type="button" onclick={() => copy("absolute")} class="btn btn-sm mr-auto gap-1.5 preset-tonal" title={path}>
      {#if copied.current === "absolute"}
        <Check size={13} />
      {:else}
        <Copy size={13} />
      {/if}
      Copy path
    </button>
    {#if hasRelative}
      <button type="button" onclick={() => copy("relative")} class="btn btn-sm gap-1.5 preset-tonal" title={relative}>
        {#if copied.current === "relative"}
          <Check size={13} />
        {:else}
          <Copy size={13} />
        {/if}
        Copy relative
      </button>
    {/if}
    <button
      type="button"
      onclick={() => void commands.revealInFinder(path).catch(() => {})}
      class="btn btn-sm gap-1.5 preset-tonal"
    >
      <FolderOpen size={13} />
      Show in Finder
    </button>
    <button
      type="button"
      onclick={() => void commands.openInZed(path).catch(() => {})}
      class="btn btn-sm gap-1.5 preset-tonal"
    >
      <SquareArrowOutUpRight size={13} />
      Open in Zed
    </button>
  {/snippet}
</DialogShell>
