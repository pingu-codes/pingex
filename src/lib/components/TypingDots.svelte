<script lang="ts">
let { label = "Codex is working" }: { label?: string } = $props();
</script>

<div class="flex items-center gap-1.5 py-1" aria-label={label}>
  <span class="typing-dot"></span>
  <span class="typing-dot" style="animation-delay: 0.18s"></span>
  <span class="typing-dot" style="animation-delay: 0.36s"></span>
</div>

<style>
  .typing-dot {
    width: 6px;
    height: 6px;
    border-radius: 9999px;
    background: color-mix(in oklab, currentColor 45%, transparent);
    animation: typing-bounce 1.1s ease-in-out infinite;
  }
  @keyframes typing-bounce {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.45;
    }
    30% {
      transform: translateY(-4px);
      opacity: 1;
    }
  }
</style>
