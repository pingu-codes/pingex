<script lang="ts" module>
  export type AddRepositorySource = "github" | "folder" | "url";
  export type AddRepositoryInitial = {
    source?: AddRepositorySource;
    /** URL mode: the repository, and whether to clone it. */
    url?: string;
    urlMode?: "prsOnly" | "clone";
    /** Folder mode for "Link existing folder…": the PRs-only repository
     *  the folder must clone, linked once it is added. */
    linkSlug?: string;
  };
</script>

<script lang="ts">
  import { Check, Search } from "@lucide/svelte";
  import { onMount } from "svelte";
  import type { DialogClose } from "$lib/app/dialogs.svelte";
  import { hostFromOption } from "$lib/app/host";
  import DialogShell from "$lib/components/DialogShell.svelte";
  import DirectoryField from "$lib/components/DirectoryField.svelte";
  import HostSelect from "$lib/components/HostSelect.svelte";
  import Segmented from "$lib/components/Segmented.svelte";
  import {
    type AddMode,
    cloneFolderName,
    defaultMode,
    githubChoices,
    parseRepositoryInput,
    plural,
    type RepositoryChoice,
    sourceOptions,
  } from "$lib/repositories/repositories";
  import { addedSlugs, ensureRepositories, refreshRepositories } from "$lib/repositories/repositoryStore.svelte";
  import { commands } from "$lib/services/homeRouting";
  import type {
    FolderFacts,
    GitHubAccount,
    GitHubRepo,
    Host,
    KnownClone,
    PrSource,
    ReviewCheckoutPolicy,
    ReviewRequests,
  } from "$lib/types";
  import { submitState } from "$lib/utils/actionState.svelte";
  import { errorMessage } from "$lib/utils/errors";

  // Add repository: from the GitHub account's lists, from a local folder whose
  // remotes are read for you, or by URL. A GitHub repository is added PRs only
  // (nothing cloned), cloned into a folder on a Host, or linked to a clone
  // Pingex already knows. A folder brings its PR source and the repository's
  // defaults (worktrees, setup command, review checkout policy).
  let {
    knownFolders,
    addFolder,
    initial = {},
    close,
  }: {
    /** Every project folder Pingex knows, to find existing clones in. */
    knownFolders: readonly string[];
    /** Add a folder as a project; `host` is null to infer it from the path. */
    addFolder: (path: string, host: Host | null) => Promise<void>;
    initial?: AddRepositoryInitial;
    close: DialogClose<true>;
  } = $props();

  // svelte-ignore state_referenced_locally
  let source = $state<AddRepositorySource>(initial.source ?? "github");
  const action = submitState();

  // --- GitHub ---------------------------------------------------------------
  let account = $state<GitHubAccount | null>(null);
  let changingAccount = $state(false);
  let mine = $state<GitHubRepo[]>([]);
  let requests = $state<ReviewRequests[]>([]);
  let clones = $state<KnownClone[]>([]);
  let added = $state<Set<string>>(new Set());
  let listsLoaded = $state(false);
  let listError = $state<string | null>(null);
  let query = $state("");
  let picks = $state<Record<string, { checked: boolean; mode: AddMode }>>({});
  let cloneInto = $state("~/code");
  let cloneHost = $state("native");

  const choices = $derived(githubChoices(mine, requests, added, clones, query));
  const picked = $derived(
    [...choices.asking, ...choices.yours].filter((choice) => !choice.added && picks[choice.slug]?.checked),
  );

  async function loadAccount() {
    account = await commands.reviewGithubAccount().catch(() => null);
  }

  async function loadLists() {
    try {
      const [, repos, asking] = await Promise.all([
        ensureRepositories(knownFolders),
        commands.reviewMyRepositories(),
        commands.reviewRepositoriesRequestingReview(),
      ]);
      added = addedSlugs();
      mine = repos;
      requests = asking;
      const candidates = [...new Set([...asking, ...repos].map((repo) => repo.slug.toLowerCase()))].filter(
        (slug) => !added.has(slug),
      );
      clones = candidates.length ? await commands.reviewFindClones(candidates, [...knownFolders]) : [];
      const all = githubChoices(repos, asking, added, clones);
      picks = Object.fromEntries([
        ...all.yours.map((choice) => [choice.slug, { checked: false, mode: defaultMode(choice) }]),
        ...all.asking.map((choice) => [choice.slug, { checked: true, mode: defaultMode(choice) }]),
      ]);
    } catch (cause) {
      listError = errorMessage(cause);
    } finally {
      listsLoaded = true;
    }
  }

  onMount(() => {
    void loadAccount();
    void loadLists();
  });

  async function reloadAccount() {
    changingAccount = false;
    await loadAccount();
    listsLoaded = false;
    await loadLists();
  }

  async function switchAccount(login: string) {
    await action.run(async () => {
      await commands.reviewSwitchGithubAccount(login);
      await reloadAccount();
    });
  }

  function cloneUrl(slug: string): string {
    return mine.find((repo) => repo.slug.toLowerCase() === slug)?.cloneUrl || `https://github.com/${slug}.git`;
  }

  function choiceDetail(choice: RepositoryChoice): string {
    const parts: string[] = [];
    if (choice.requests > 0) parts.push(plural(choice.requests, "review request"));
    if (choice.clone) parts.push(`clone found at ${choice.clone.path}`);
    else if (choice.requests > 0) parts.push("not cloned");
    if (choice.fork) parts.push(`fork of ${choice.fork}`);
    return parts.join(" · ");
  }

  async function addFromGitHub() {
    const host = hostFromOption(cloneHost);
    const linked: string[] = [];
    for (const choice of picked) {
      const mode = picks[choice.slug].mode;
      if (mode === "link" && choice.clone) {
        await addFolder(choice.clone.path, null);
        await commands.reviewLinkClone(choice.clone.path, choice.slug);
        linked.push(choice.clone.path);
      } else if (mode === "clone") {
        const path = await commands.reviewCloneRepository(
          cloneInto,
          cloneUrl(choice.slug),
          cloneFolderName(choice.slug),
          host,
        );
        await addFolder(path, host);
        linked.push(path);
      } else {
        await commands.reviewAddPrsOnlyRepository(choice.slug);
      }
      // Added: later failures leave it out of the next attempt.
      picks[choice.slug].checked = false;
      added = new Set([...added, choice.slug]);
    }
    await refreshRepositories(linked);
  }

  // --- Local folder -----------------------------------------------------------
  let folder = $state("");
  let folderHost = $state("native");
  let folderHostChosen = $state(false);
  let facts = $state<FolderFacts | null>(null);
  let inspected = $state("");
  let folderError = $state<string | null>(null);
  let prSource = $state<PrSource>("origin");
  let worktreeDir = $state("");
  let setupCommand = $state("");
  let policy = $state<ReviewCheckoutPolicy>("ask");

  const sources = $derived(facts ? sourceOptions(facts.remotes) : []);
  const policyOptions = [
    { value: "ask", label: "Ask first" },
    { value: "whenNeeded", label: "Create when needed" },
    { value: "never", label: "Never" },
  ] as const;

  /** A folder picked inside a distribution runs there unless a Host was chosen. */
  function folderTyped(value: string) {
    folder = value;
    if (!folderHostChosen) folderHost = /^\\\\(?:wsl\.localhost|wsl\$)\\([^\\]+)/i.exec(value)?.[1] ?? "native";
  }

  async function inspect() {
    const path = folder.trim();
    if (!path || path === inspected) return;
    inspected = path;
    folderError = null;
    facts = null;
    try {
      const read = await commands.reviewInspectFolder(path, hostFromOption(folderHost));
      const stored = await commands.reviewRepositorySettings(read.root).catch(() => null);
      facts = read;
      prSource = read.prSource;
      const known = stored && stored.updatedAt > 0 ? stored : null;
      setupCommand = known?.setupCommand ?? read.setupCommand ?? "";
      worktreeDir = known?.worktreeDir ?? "";
      policy = known?.reviewCheckoutPolicy ?? "ask";
    } catch (cause) {
      folderError = errorMessage(cause);
    }
  }

  async function addFromFolder() {
    if (!facts) return;
    const linkSlug = initial.linkSlug?.toLowerCase();
    if (linkSlug && !facts.remotes.some((remote) => remote.slug === linkSlug))
      throw new Error(`This folder has no remote for ${linkSlug}`);
    const host = hostFromOption(folderHost);
    await addFolder(facts.root, host);
    const current = await commands.reviewRepositorySettings(facts.root);
    await commands.reviewSaveRepositorySettings(facts.root, {
      ...current,
      prSource,
      setupCommand: setupCommand.trim() || null,
      worktreeDir: worktreeDir.trim() || null,
      reviewCheckoutPolicy: policy,
    });
    if (linkSlug) await commands.reviewLinkClone(facts.root, linkSlug);
    // A folder that clones a PRs-only repository replaces it.
    const repository = await commands.reviewRepository(facts.root);
    for (const feed of repository.feeds) await commands.reviewRemovePrsOnlyRepository(feed.slug);
    await refreshRepositories([facts.root]);
  }

  // --- URL ------------------------------------------------------------------
  // svelte-ignore state_referenced_locally
  let url = $state(initial.url ?? "");
  // svelte-ignore state_referenced_locally
  let urlMode = $state<"prsOnly" | "clone">(initial.urlMode ?? "prsOnly");
  const parsed = $derived(parseRepositoryInput(url));

  async function addFromUrl() {
    if (!parsed) return;
    if (urlMode === "clone") {
      const host = hostFromOption(cloneHost);
      const path = await commands.reviewCloneRepository(cloneInto, parsed.cloneUrl, parsed.name, host);
      await addFolder(path, host);
      await refreshRepositories([path]);
    } else {
      await commands.reviewAddPrsOnlyRepository(parsed.slug);
      await refreshRepositories([]);
    }
  }

  // --- Footer -----------------------------------------------------------------
  const submitLabel = $derived(
    source === "github"
      ? picked.length > 0
        ? `Add ${picked.length === 1 ? "1 repository" : `${picked.length} repositories`}`
        : "Add repositories"
      : "Add repository",
  );
  const canSubmit = $derived(
    !action.busy &&
      (source === "github"
        ? picked.length > 0
        : source === "folder"
          ? facts !== null && !folderError
          : parsed !== null),
  );

  async function submit() {
    if (!canSubmit) return;
    const work = source === "github" ? addFromGitHub : source === "folder" ? addFromFolder : addFromUrl;
    if (await action.run(work)) close(true);
  }
</script>

{#snippet choiceRow(choice: RepositoryChoice)}
  {#if choice.added}
    <div class="menu-row" data-repository={choice.slug}>
      <Check size={14} class="shrink-0 text-success-500" />
      <span class="min-w-0 flex-1 truncate">{choice.slug}</span>
      <span class="text-surface-500">Added</span>
    </div>
  {:else if picks[choice.slug]}
    {@const pick = picks[choice.slug]}
    <div class="menu-row" class:bg-surface-200-800={pick.checked} data-repository={choice.slug}>
      <label class="flex min-w-0 flex-1 items-center gap-2">
        <input
          type="checkbox"
          class="checkbox"
          checked={pick.checked}
          onchange={(event) => (pick.checked = event.currentTarget.checked)}
        >
        <span class="flex min-w-0 flex-1 flex-col">
          <span class="truncate font-medium">{choice.slug}</span>
          {#if choiceDetail(choice)}
            <span class="truncate text-surface-500">{choiceDetail(choice)}</span>
          {/if}
        </span>
      </label>
      {#if choice.clone}
        <span class="status-badge preset-tonal-success">Link clone</span>
      {:else}
        <Segmented
          value={pick.mode}
          options={[
            { value: "prsOnly", label: "PRs only" },
            { value: "clone", label: "Clone" },
          ]}
          label={`How to add ${choice.slug}`}
          onChange={(mode) => (pick.mode = mode)}
        />
      {/if}
    </div>
  {/if}
{/snippet}

{#snippet cloneSettings(note: boolean)}
  <div class="panel-card flex flex-col gap-2 px-3 py-2.5 text-xs">
    <div class="flex flex-wrap items-center gap-2">
      <label for="add-repository-clone-into" class="w-20 font-medium">Clone into</label>
      <input
        id="add-repository-clone-into"
        class="field min-w-40 flex-1 font-mono text-[11px]"
        bind:value={cloneInto}
        spellcheck="false"
      >
      <label for="add-repository-clone-host" class="sr-only">Host</label>
      <HostSelect id="add-repository-clone-host" bind:value={cloneHost} class="field w-auto text-xs" />
    </div>
    {#if note}
      <p class="text-surface-500">
        PRs only uses no disk. You can review, comment and run diff reviews, and clone later when something has to run.
      </p>
    {/if}
  </div>
{/snippet}

<DialogShell title="Add repository" width={560} onClose={() => close()}>
  <div class="mt-1 flex flex-col gap-3.5 text-xs">
    <div class="flex flex-col gap-2">
      <p data-testid="github-account" class="text-xs leading-5 text-surface-500">
        {#if account?.login}
          GitHub: <b class="font-medium text-surface-700-300">{account.login}</b>, using the token from
          <span class="font-mono">gh</span>{account.tokenSource ? ` (${account.tokenSource})` : ""}
          ·
          <button type="button" class="anchor" onclick={() => (changingAccount = !changingAccount)}>Change</button>
        {:else if account}
          GitHub isn't connected. Run <span class="font-mono">gh auth login</span> in a terminal, then
          <button type="button" class="anchor" onclick={() => void reloadAccount()}>refresh</button>.
        {:else}
          Reading the GitHub account…
        {/if}
      </p>
      {#if changingAccount && account}
        <div class="panel-card flex flex-col gap-2 px-3 py-2.5">
          {#each account.logins.filter((login) => login !== account?.login) as login (login)}
            <button type="button" class="btn btn-sm self-start hover:preset-tonal" onclick={() => switchAccount(login)}>
              Use {login}
            </button>
          {/each}
          <p class="text-surface-500">
            To use another account, run <span class="font-mono">gh auth login</span> in a terminal, then
            <button type="button" class="anchor" onclick={() => void reloadAccount()}>refresh</button>.
          </p>
        </div>
      {/if}
    </div>

    <Segmented
      value={source}
      options={[
        { value: "github", label: "GitHub" },
        { value: "folder", label: "Local folder" },
        { value: "url", label: "URL" },
      ]}
      label="Source"
      onChange={(next) => (source = next)}
    />

    {#if source === "github"}
      <label class="relative flex items-center">
        <Search size={14} class="pointer-events-none absolute left-2 text-surface-500" />
        <span class="sr-only">Search repositories</span>
        <input
          type="search"
          class="field w-full pl-7 text-xs"
          placeholder="Search your repositories and organizations"
          bind:value={query}
        >
      </label>

      {#if !listsLoaded}
        <div class="placeholder h-16 animate-pulse rounded-xl" aria-label="Loading repositories"></div>
      {:else if listError}
        <p role="alert" class="error-note">{listError}</p>
      {:else}
        {#if choices.asking.length > 0}
          <div class="flex flex-col gap-1.5">
            <div class="flex items-center gap-2">
              <span id="add-repository-asking" class="eyebrow flex-1">Asking for your review</span>
              <span class="text-surface-500">Not added yet</span>
            </div>
            <div role="group" aria-labelledby="add-repository-asking" class="panel-card flex flex-col gap-0.5 p-1">
              {#each choices.asking as choice (choice.slug)}
                {@render choiceRow(choice)}
              {/each}
            </div>
          </div>
        {/if}
        <div class="flex flex-col gap-1.5">
          <span id="add-repository-yours" class="eyebrow">Your repositories</span>
          <div role="group" aria-labelledby="add-repository-yours" class="panel-card flex flex-col gap-0.5 p-1">
            {#each choices.yours as choice (choice.slug)}
              {@render choiceRow(choice)}
            {:else}
              <p class="px-2 py-1.5 text-surface-500">{query ? "No repositories match." : "No repositories found."}</p>
            {/each}
          </div>
        </div>
      {/if}

      {@render cloneSettings(true)}
      <p class="text-surface-500">
        Repositories waiting on your review come first. If one is already cloned somewhere Pingex knows about, it's
        linked instead of cloned again.
      </p>
    {:else if source === "folder"}
      <div class="flex flex-wrap items-end gap-2">
        <div class="flex min-w-0 flex-1 flex-col gap-1" onfocusout={() => void inspect()}>
          <label for="add-repository-folder" class="font-medium">Folder</label>
          <DirectoryField
            id="add-repository-folder"
            value={folder}
            onInput={folderTyped}
            onPick={(value) => {
              folderTyped(value);
              void inspect();
            }}
            placeholder="Choose a folder"
            browseLabel="Browse for a repository folder"
            pickerTitle="Choose a repository folder"
          />
        </div>
        <div class="flex flex-col gap-1">
          <label for="add-repository-folder-host" class="font-medium">Host</label>
          <HostSelect
            id="add-repository-folder-host"
            bind:value={folderHost}
            class="field w-auto text-xs"
            onchange={() => {
              folderHostChosen = true;
              inspected = "";
              void inspect();
            }}
          />
        </div>
      </div>

      {#if initial.linkSlug}
        <p class="text-surface-500">
          Choose a folder that clones <span class="font-mono">{initial.linkSlug}</span>. It replaces the PRs-only
          repository.
        </p>
      {/if}
      {#if folderError}
        <p role="alert" class="error-note">{folderError}</p>
      {:else if facts}
        <div
          data-testid="folder-facts"
          class="panel-card flex flex-wrap gap-x-4 gap-y-1.5 px-3 py-2.5 text-surface-600-400"
        >
          <span>Git repository</span>
          {#if facts.branch}
            <span>·</span><span>on <span class="font-mono">{facts.branch}</span></span>
          {/if}
          {#if facts.linkedWorktrees > 0}
            <span>·</span><span>{plural(facts.linkedWorktrees, "linked worktree")}</span>
          {/if}
          {#if facts.lockfile}
            <span>·</span><span><span class="font-mono">{facts.lockfile}</span> found</span>
          {/if}
        </div>

        <div class="flex flex-col gap-1.5">
          <span id="add-repository-source" class="eyebrow">Pull requests from</span>
          <div role="radiogroup" aria-labelledby="add-repository-source" class="panel-card flex flex-col gap-0.5 p-1">
            {#each sources as option (option.value)}
              <label class="menu-row" class:bg-surface-200-800={prSource === option.value}>
                <input
                  type="radio"
                  class="radio"
                  name="add-repository-source"
                  value={option.value}
                  checked={prSource === option.value}
                  onchange={() => (prSource = option.value)}
                >
                <span class="flex min-w-0 flex-1 flex-col">
                  <span>
                    {#if option.slug}
                      <span class="font-mono">{option.label}</span>
                      · {option.slug}
                    {:else}
                      {option.label}
                    {/if}
                  </span>
                  {#if option.description}
                    <span class="text-surface-500">{option.description}</span>
                  {/if}
                </span>
              </label>
            {/each}
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <span class="eyebrow">Defaults for this repository</span>
          <div class="panel-card divide-y divide-surface-200-800">
            <div class="flex flex-wrap items-center gap-2 px-3 py-2">
              <label for="add-repository-worktrees" class="w-32 font-medium">Worktrees in</label>
              <input
                id="add-repository-worktrees"
                class="field min-w-40 flex-1 font-mono text-[11px]"
                placeholder="App default"
                spellcheck="false"
                bind:value={worktreeDir}
              >
            </div>
            <div class="flex flex-wrap items-center gap-2 px-3 py-2">
              <label for="add-repository-setup" class="w-32 font-medium">Setup command</label>
              <input
                id="add-repository-setup"
                class="field min-w-40 flex-1 font-mono text-[11px]"
                placeholder="None"
                spellcheck="false"
                bind:value={setupCommand}
              >
            </div>
            <div class="flex flex-wrap items-center gap-2 px-3 py-2">
              <span class="w-32 font-medium">Review checkouts</span>
              <Segmented
                value={policy}
                options={policyOptions}
                label="Review checkouts"
                onChange={(next) => (policy = next)}
              />
            </div>
          </div>
        </div>
        <p class="text-surface-500">
          A fork defaults to its upstream, where its PRs open. Linked worktrees nest under the repository automatically
          and aren't added one by one.
        </p>
      {/if}
    {:else}
      <div class="flex flex-col gap-1">
        <label for="add-repository-url" class="font-medium">Repository URL</label>
        <input
          id="add-repository-url"
          class="field font-mono text-[11px]"
          placeholder="https://github.com/owner/name or owner/name"
          spellcheck="false"
          bind:value={url}
        >
        {#if url.trim() && !parsed}
          <p class="text-surface-500">Enter a GitHub repository as a URL or owner/name.</p>
        {/if}
      </div>
      <Segmented
        value={urlMode}
        options={[
          { value: "prsOnly", label: "PRs only" },
          { value: "clone", label: "Clone" },
        ]}
        label="How to add it"
        onChange={(next) => (urlMode = next)}
      />
      {#if urlMode === "clone"}
        {@render cloneSettings(false)}
      {:else}
        <p class="text-surface-500">PRs only uses no disk. You can clone later when something has to run.</p>
      {/if}
    {/if}

    {#if action.error}
      <p role="alert" class="error-note">{action.error}</p>
    {/if}
  </div>
  {#snippet footer()}
    <button type="button" class="btn btn-sm hover:preset-tonal" onclick={() => close()}>Cancel</button>
    <button type="button" class="btn btn-sm preset-filled-primary-500" disabled={!canSubmit} onclick={submit}>
      {action.busy ? "Adding…" : submitLabel}
    </button>
  {/snippet}
</DialogShell>
