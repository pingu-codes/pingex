<!--
  One of the inbox's people pickers (Author, Assignee): a pill that reads
  "Author", or "Author: jo-r +1" once someone is chosen, and opens a list to
  tick people in. Typing narrows the list by login or name; Enter ticks the
  highlighted person and the list stays open for the next. Each row counts the
  PRs that person would add. The × clears the picker.
-->
<script lang="ts">
  import { Check, ChevronDown, CircleDashed, X } from "@lucide/svelte";
  import { Combobox, type ComboboxRootProps, Portal, useListCollection } from "@skeletonlabs/skeleton-svelte";
  import { type People, type PersonOption, UNASSIGNED } from "$lib/inbox/inbox";
  import LoginAvatar from "$lib/inbox/LoginAvatar.svelte";

  let {
    label,
    options,
    chosen,
    viewer,
    people,
    onChange,
  }: {
    /** "Author" or "Assignee". */
    label: string;
    options: PersonOption[];
    chosen: string[];
    viewer: string;
    people: People;
    onChange: (logins: string[]) => void;
  } = $props();

  let open = $state(false);
  let query = $state("");

  function shortName(login: string): string {
    if (login === UNASSIGNED) return "Unassigned";
    return login === viewer ? "you" : login;
  }

  const summary = $derived(
    chosen.length === 0
      ? label
      : `${label}: ${shortName(chosen[0])}${chosen.length > 1 ? ` +${chosen.length - 1}` : ""}`,
  );

  const items = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return options;
    return options.filter(({ login }) => {
      const words = [
        login === UNASSIGNED ? "unassigned" : login,
        people.get(login)?.name ?? "",
        login === viewer ? "you" : "",
      ];
      return words.some((word) => word.toLowerCase().includes(needle));
    });
  });

  const collection = $derived(
    useListCollection({
      items,
      itemToString: (option) => shortName(option.login),
      itemToValue: (option) => option.login,
    }),
  );

  const onOpenChange: ComboboxRootProps["onOpenChange"] = (details) => {
    open = details.open;
    if (!details.open) query = "";
  };

  const onInputValueChange: ComboboxRootProps["onInputValueChange"] = (details) => {
    query = details.inputValue;
  };

  const onValueChange: ComboboxRootProps["onValueChange"] = (details) => {
    onChange(details.value);
  };
</script>

<Combobox
  {collection}
  multiple
  value={chosen}
  inputValue={query}
  placeholder={summary}
  {onValueChange}
  {onInputValueChange}
  {onOpenChange}
  openOnClick
  inputBehavior="autohighlight"
  positioning={{ placement: "bottom-start", gutter: 4, sameWidth: false }}
  translations={{ clearTriggerLabel: `Clear the ${label.toLowerCase()} filter` }}
  class="w-auto gap-0"
>
  <Combobox.Control
    class="{chosen.length > 0
      ? "pill-active"
      : "pill"} gap-0.5 focus-within:outline-2 focus-within:outline-offset-1 focus-within:outline-primary-500"
  >
    <Combobox.Input
      aria-label={summary}
      size={Math.max(summary.length, query.length, 6)}
      class="h-auto w-auto min-w-0 cursor-pointer rounded-none border-0 bg-transparent p-0 text-[11px] text-inherit shadow-none ring-0 outline-none placeholder:text-inherit"
    />
    <Combobox.ClearTrigger
      class="static -mr-0.5 h-auto min-h-0 w-auto translate-y-0 rounded-full bg-transparent p-0 text-inherit hover:bg-black/15"
    >
      <X size={11} aria-hidden="true" />
    </Combobox.ClearTrigger>
    {#if chosen.length === 0}
      <Combobox.Trigger
        aria-label="Show people"
        class="static -mr-0.5 size-auto min-h-0 translate-y-0 bg-transparent p-0 text-inherit"
      >
        <ChevronDown size={11} aria-hidden="true" />
      </Combobox.Trigger>
    {/if}
  </Combobox.Control>
  <Portal>
    <Combobox.Positioner>
      <Combobox.Content
        class="popover-card z-50 max-h-80 min-w-64 max-w-sm select-none gap-0 overflow-y-auto p-1 text-xs"
      >
        <!-- Rows only while open: two closed pickers needn't load every avatar. -->
        {#if open}
          {#each items as option (option.login)}
            {@const name = people.get(option.login)?.name}
            <Combobox.Item
              item={option}
              class="menu-row cursor-pointer data-[state=checked]:bg-transparent data-[state=checked]:text-inherit data-[state=checked]:data-highlighted:preset-tonal"
            >
              {#if option.login === UNASSIGNED}
                <CircleDashed size={18} class="shrink-0 text-surface-500" aria-hidden="true" />
              {:else}
                <LoginAvatar login={option.login} person={people.get(option.login)} plain />
              {/if}
              <span class="flex min-w-0 flex-1 items-baseline gap-1.5">
                <Combobox.ItemText class="truncate"
                  >{option.login === UNASSIGNED ? "Unassigned" : option.login}{option.login === viewer
                    ? " (you)"
                    : ""}</Combobox.ItemText
                >
                {#if name}
                  <span class="truncate text-[11px] text-surface-500">{name}</span>
                {/if}
              </span>
              <span class="shrink-0 text-[11px] text-surface-500 tabular-nums">{option.count}</span>
              <span class="flex w-3.5 shrink-0 justify-center">
                <Combobox.ItemIndicator>
                  <Check size={13} class="text-primary-500" />
                </Combobox.ItemIndicator>
              </span>
            </Combobox.Item>
          {:else}
            <div class="px-2 py-1.5 text-surface-500">No one matches</div>
          {/each}
        {/if}
      </Combobox.Content>
    </Combobox.Positioner>
  </Portal>
</Combobox>
