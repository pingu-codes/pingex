<script lang="ts">
  import { type GitError, gitErrorHints } from "./gitErrors";

  let { error, class: className = "" }: { error: GitError; class?: string } = $props();

  const hint = $derived(gitErrorHints[error.kind]);
</script>

<div class="error-note {className}">
  <div>{error.message}</div>
  {#if hint}
    <div class="mt-0.5 opacity-80">{hint}</div>
  {/if}
  {#if error.detail}
    <pre
      class="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[10px] leading-4 opacity-90"
    >{error.detail}</pre>
  {/if}
</div>
