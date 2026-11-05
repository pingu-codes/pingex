<!--
  The inline box for a new comment or suggestion on selected lines. What it
  adds goes into the pending review, never straight to GitHub.
-->
<script lang="ts">
  let {
    label,
    mode,
    initialReplacement = "",
    onAdd,
    onCancel,
  }: {
    /** "Line 7" or "Lines 3–5". */
    label: string;
    mode: "comment" | "suggestion";
    /** The selected lines, for a suggestion to start from. */
    initialReplacement?: string;
    onAdd: (body: string, replacement: string | null) => void;
    onCancel: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally -- the composer starts from the selection once.
  let replacement = $state(initialReplacement);
  let body = $state("");
  const lower = $derived(label.charAt(0).toLowerCase() + label.slice(1));
  const ready = $derived(mode === "suggestion" || body.trim().length > 0);

  function add() {
    if (!ready) return;
    onAdd(body.trim(), mode === "suggestion" ? replacement : null);
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onCancel();
    } else if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      // ⌘↵ adds this comment; it must not also submit the whole review.
      event.preventDefault();
      event.stopPropagation();
      add();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -- Esc and ⌘↵ for the fields inside. -->
<div
  role="group"
  aria-label="New {mode === "suggestion" ? "suggestion" : "comment"} on {lower}"
  class="my-2 mr-3 ml-[104px] flex flex-col gap-2 rounded-container border border-primary-500/40 bg-surface-100-900 p-2.5 text-xs"
  onkeydown={keydown}
>
  {#if mode === "suggestion"}
    <!-- svelte-ignore a11y_autofocus -->
    <textarea
      aria-label="Suggested change for {lower}"
      bind:value={replacement}
      rows={Math.max(2, replacement.split("\n").length)}
      class="field w-full resize-y font-mono text-xs"
      autofocus
    ></textarea>
    <textarea
      aria-label="Comment on {lower}"
      bind:value={body}
      rows="2"
      placeholder="Why (optional)"
      class="field w-full resize-none text-[13px]"
    ></textarea>
  {:else}
    <!-- svelte-ignore a11y_autofocus -->
    <textarea
      aria-label="Comment on {lower}"
      bind:value={body}
      rows="3"
      placeholder="Leave a comment"
      class="field w-full resize-y text-[13px]"
      autofocus
    ></textarea>
  {/if}
  <div class="flex justify-end gap-1.5">
    <button type="button" class="btn btn-sm hover:preset-tonal text-[11px]" onclick={onCancel}>Cancel</button>
    <button type="button" class="btn btn-sm preset-tonal text-[11px]" disabled={!ready} onclick={add}>
      Add to review
    </button>
  </div>
</div>
