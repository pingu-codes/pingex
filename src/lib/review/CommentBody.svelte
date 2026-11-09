<script lang="ts">
  // A GitHub comment, review or PR body rendered as markdown. Links go to the
  // browser through the app-wide handler in `externalLinks.ts`.
  import { copyCode } from "$lib/utils/copy";
  import { renderMarkdown } from "$lib/utils/markdown";

  let { body, class: className = "" }: { body: string; class?: string } = $props();

  const html = $derived(renderMarkdown(body));
</script>

<div class="comment-body prose-md prose-md-compact text-[13px] leading-5 {className}" use:copyCode>
  {@html html}
</div>
