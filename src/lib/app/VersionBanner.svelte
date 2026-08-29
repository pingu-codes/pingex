<script lang="ts">
import { X } from "@lucide/svelte";
import { bannerText, codexVersion, dismissVersionBanner } from "$lib/app/codexVersion.svelte";
import TooltipButton from "$lib/components/TooltipButton.svelte";

const banner = $derived(codexVersion.banner);
</script>

{#if banner && !banner.dismissed}
  <div
    role="status"
    class="flex items-center gap-2 border-b border-surface-200-800 px-4 py-1.5 text-xs {banner.tier === 'older' ? 'preset-tonal-warning' : 'preset-tonal'}"
  >
    <span class="min-w-0 flex-1 truncate" title={bannerText(banner)}>{bannerText(banner)}</span>
    <a class="shrink-0 underline opacity-80 hover:opacity-100" href="https://github.com/pingu-codes/pingex/blob/main/docs/SUPPORTED_VERSIONS.md" target="_blank" rel="noreferrer">Supported versions</a>
    <TooltipButton label="Dismiss" onclick={dismissVersionBanner} aria-label="Dismiss version warning" class="btn-icon btn-icon-sm text-surface-500 hover:preset-tonal"><X size={13} /></TooltipButton>
  </div>
{/if}
