<!--
  The Pull requests heading when there are several repositories: a combobox
  that shows the current repository's name and, once opened, filters the
  repositories by name or folder as you type. Enter picks the first match.
-->
<script lang="ts">
  import { Check, ChevronDown } from "@lucide/svelte";
  import { Combobox, type ComboboxRootProps, Portal, useListCollection } from "@skeletonlabs/skeleton-svelte";
  import { untrack } from "svelte";

  type Repo = { path: string; name: string };

  let {
    repos,
    repoPath,
    onSelectRepo,
  }: {
    repos: Repo[];
    repoPath: string;
    onSelectRepo: (path: string) => void;
  } = $props();

  const currentName = $derived(repos.find((repo) => repo.path === repoPath)?.name ?? repoPath);

  // Opening empties the input so typing starts a fresh filter; the name stays
  // as the placeholder, and closing without a pick puts it back.
  let inputValue = $state(untrack(() => currentName));
  let query = $state("");

  const items = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return repos;
    return repos.filter((repo) => repo.name.toLowerCase().includes(needle) || repo.path.toLowerCase().includes(needle));
  });

  const collection = $derived(
    useListCollection({
      items,
      itemToString: (repo) => repo.name,
      itemToValue: (repo) => repo.path,
    }),
  );

  const onOpenChange: ComboboxRootProps["onOpenChange"] = (details) => {
    query = "";
    if (details.open) inputValue = "";
    else if (details.reason !== "item-select") inputValue = currentName;
  };

  const onInputValueChange: ComboboxRootProps["onInputValueChange"] = (details) => {
    inputValue = details.inputValue;
    query = details.reason === "input-change" ? details.inputValue : "";
  };

  const onValueChange: ComboboxRootProps["onValueChange"] = (details) => {
    const next = details.value[0];
    if (next && next !== repoPath) onSelectRepo(next);
  };
</script>

<Combobox
  {collection}
  value={[repoPath]}
  {inputValue}
  placeholder={currentName}
  {onValueChange}
  {onInputValueChange}
  {onOpenChange}
  openOnClick
  inputBehavior="autohighlight"
  positioning={{ placement: "bottom-start", gutter: 4 }}
>
  <Combobox.Control
    class="-mx-1 flex w-fit items-center gap-1 rounded px-1 hover:preset-tonal focus-within:preset-tonal"
  >
    <Combobox.Input
      aria-label="Repository"
      size={Math.max(currentName.length, inputValue.length, 8)}
      class="min-w-0 bg-transparent font-semibold outline-none placeholder:text-surface-500"
    />
    <Combobox.Trigger aria-label="Show repositories" class="text-surface-500">
      <ChevronDown size={14} aria-hidden="true" />
    </Combobox.Trigger>
  </Combobox.Control>
  <Portal>
    <Combobox.Positioner>
      <Combobox.Content
        class="popover-card z-50 max-h-80 min-w-64 max-w-md select-none overflow-y-auto p-1 text-xs font-normal"
      >
        {#each items as repo (repo.path)}
          <Combobox.Item item={repo} class="menu-row cursor-pointer">
            <span class="flex min-w-0 flex-1 flex-col">
              <Combobox.ItemText class="truncate">{repo.name}</Combobox.ItemText>
              <span class="truncate text-[11px] text-surface-500" title={repo.path}>{repo.path}</span>
            </span>
            <Combobox.ItemIndicator>
              <Check size={13} class="text-primary-500" />
            </Combobox.ItemIndicator>
          </Combobox.Item>
        {:else}
          <div class="px-2 py-1.5 text-surface-500">No matching repositories</div>
        {/each}
      </Combobox.Content>
    </Combobox.Positioner>
  </Portal>
</Combobox>
