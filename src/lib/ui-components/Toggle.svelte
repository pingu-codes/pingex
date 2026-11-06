<script lang="ts">
  import { Switch } from "@skeletonlabs/skeleton-svelte";

  // An on/off switch whose label is only read by screen readers; the visible
  // title and description sit beside it in the caller's row. Skeleton renders a
  // visually hidden checkbox, which gets `role="switch"` so it reads as one.
  let {
    checked,
    label,
    onChange,
    disabled = false,
    testId,
  }: {
    checked: boolean;
    /** Accessible name of the switch. */
    label: string;
    onChange: (checked: boolean) => void;
    disabled?: boolean;
    testId?: string;
  } = $props();
</script>

<Switch {checked} {disabled} onCheckedChange={(details) => onChange(details.checked)} class="shrink-0">
  <Switch.Control class="h-5 w-9">
    <Switch.Thumb />
  </Switch.Control>
  <Switch.Label class="sr-only">{label}</Switch.Label>
  <Switch.HiddenInput role="switch" data-testid={testId} />
</Switch>
