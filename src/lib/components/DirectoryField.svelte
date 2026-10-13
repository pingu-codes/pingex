<script lang="ts">
import { FolderOpen } from "@lucide/svelte";
import { open as openFolderPicker } from "@tauri-apps/plugin-dialog";
import TooltipButton from "$lib/components/TooltipButton.svelte";
import { isTauri } from "$lib/services/api";

// A path input with a native folder picker beside it. The picker only exists
// in the desktop app, so the browser preview gets the plain input.
let {
  value,
  onInput,
  onPick = onInput,
  browseLabel,
  pickerTitle,
  placeholder,
  id,
  ariaLabel,
  inputClass = "field font-mono text-[13px]",
  buttonSize = "btn-sm",
}: {
  value: string;
  /** Called as the user types. */
  onInput: (value: string) => void;
  /** Called with the folder the picker returned; defaults to `onInput`. */
  onPick?: (value: string) => void;
  /** Tooltip and accessible name of the Browse button. */
  browseLabel: string;
  /** Title of the native folder picker. */
  pickerTitle: string;
  placeholder?: string;
  id?: string;
  ariaLabel?: string;
  /** Look of the input: the compact `field` by default, or Skeleton's `input`. */
  inputClass?: string;
  /** Size of the Browse button, to match the input's height. */
  buttonSize?: "btn-sm" | "btn-base";
} = $props();

async function browse() {
  if (!isTauri()) return;
  const picked = await openFolderPicker({ directory: true, multiple: false, title: pickerTitle });
  if (typeof picked === "string") onPick(picked);
}
</script>

<div class="flex gap-2">
  <input
    type="text"
    {id}
    {value}
    {placeholder}
    aria-label={ariaLabel}
    oninput={(event) => onInput(event.currentTarget.value)}
    class="min-w-0 flex-1 {inputClass}"
    autocomplete="off"
    autocorrect="off"
    autocapitalize="off"
    spellcheck={false}
  />
  {#if isTauri()}
    <TooltipButton label={browseLabel} type="button" onclick={browse} class="btn {buttonSize} preset-tonal shrink-0" aria-label={browseLabel}>
      <FolderOpen size={14} />
    </TooltipButton>
  {/if}
</div>
