<!-- A project's override for one integration: inherit, or force it on or off. -->
<script lang="ts">
import type { OverrideValue } from "$lib/integrations/integrationsHelpers";

let {
  label,
  value,
  inheritLabel,
  disabled = false,
  class: className = "",
  onChange,
}: {
  /** Accessible name, e.g. "Setting for github". */
  label: string;
  value: OverrideValue;
  inheritLabel: string;
  disabled?: boolean;
  class?: string;
  onChange: (value: OverrideValue) => void;
} = $props();
</script>

<select
  aria-label={label}
  class="select text-xs {className}"
  {disabled}
  {value}
  onchange={(event) => onChange(event.currentTarget.value as OverrideValue)}
>
  <option value="inherit">{inheritLabel}</option>
  <option value="enabled">Enabled</option>
  <option value="disabled">Disabled</option>
</select>
