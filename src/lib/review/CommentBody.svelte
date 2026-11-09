<script lang="ts">
  // A GitHub comment, review or PR body rendered as markdown. Links go to the
  // browser through the app-wide handler in `externalLinks.ts`.
  import { copyCode } from "$lib/utils/copy";
  import { renderMarkdown } from "$lib/utils/markdown";

  let {
    body,
    class: className = "",
    findId,
  }: {
    body: string;
    class?: string;
    /** Its unit in Cmd+F's search (`reviewFind.ts`), where it has one. */
    findId?: string;
  } = $props();

  const html = $derived(renderMarkdown(body));
</script>

<div class="comment-body prose-md prose-md-compact text-[13px] leading-5 {className}" data-find={findId} use:copyCode>
  {@html html}
</div>
