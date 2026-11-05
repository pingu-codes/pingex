<script lang="ts">
  import { Cloud, Ellipsis } from "@lucide/svelte";
  import ActionMenu from "$lib/components/ActionMenu.svelte";
  import type { MenuEntry } from "$lib/components/actionMenu";
  import type { AddRepositoryInitial } from "$lib/repositories/AddRepositoryDialog.svelte";
  import { refreshPrsOnly, repositoryStore } from "$lib/repositories/repositoryStore.svelte";
  import { commands } from "$lib/services/homeRouting";
  import { toastError } from "$lib/toaster";
  import type { Repository } from "$lib/types";
  import { errorMessage } from "$lib/utils/errors";

  // The sidebar's PRs-only repositories: GitHub repositories with nothing
  // cloned, drawn with a cloud. Their menu upgrades one to a clone (Clone…,
  // Link existing folder…) or removes it.
  let { onAddRepository }: { onAddRepository: (initial?: AddRepositoryInitial) => void } = $props();

  const entries: MenuEntry[] = [
    { id: "clone", label: "Clone…" },
    { id: "link", label: "Link existing folder…" },
    { kind: "separator" },
    { id: "remove", label: "Remove", danger: true },
  ];

  async function run(repository: Repository, id: string) {
    if (id === "clone") onAddRepository({ source: "url", url: repository.slug, urlMode: "clone" });
    else if (id === "link") onAddRepository({ source: "folder", linkSlug: repository.slug });
    else if (id === "remove") {
      try {
        await commands.reviewRemovePrsOnlyRepository(repository.slug);
        await refreshPrsOnly();
      } catch (cause) {
        toastError(`Could not remove ${repository.slug}: ${errorMessage(cause)}`);
      }
    }
  }
</script>

{#if repositoryStore.prsOnly.length > 0}
  <div role="group" aria-label="PRs only" class="mt-1 space-y-0.5">
    {#each repositoryStore.prsOnly as repository (repository.key)}
      <ActionMenu
        {entries}
        label={`Repository ${repository.slug}`}
        buttonLabel="Repository menu"
        onSelect={(id) => void run(repository, id)}
      >
        {#snippet row({ rowProps, buttonProps })}
          <div
            {...rowProps}
            data-repository={repository.slug}
            title={repository.slug}
            class="group/repository relative flex items-center gap-2 rounded-md py-1.5 pl-2 pr-8 text-[13px] font-medium hover:preset-tonal"
          >
            <span class="grid w-3.5 place-items-center"></span>
            <span
              role="img"
              aria-label="PRs only, not cloned"
              class="grid shrink-0 place-items-center text-surface-500"
            >
              <Cloud size={15} strokeWidth={1.7} />
            </span>
            <span class="min-w-0 flex-1 truncate">{repository.name}</span>
            <button
              type="button"
              {...buttonProps}
              class="absolute right-1 top-1/2 size-6 -translate-y-1/2 row-action group-hover/repository:opacity-100"
            >
              <Ellipsis size={14} />
            </button>
          </div>
        {/snippet}
      </ActionMenu>
    {/each}
  </div>
{/if}
