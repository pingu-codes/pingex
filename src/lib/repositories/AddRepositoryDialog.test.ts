import { render, screen, waitFor, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { NATIVE_HOST } from "$lib/app/host";
import AddRepositoryDialog, { type AddRepositoryInitial } from "$lib/repositories/AddRepositoryDialog.svelte";
import { resetRepositories } from "$lib/repositories/repositoryStore.svelte";
import { commands } from "$lib/services/homeRouting";
import { FORK_FOLDER, resetPreviewRepositories } from "$lib/services/preview/repositories";

// Add repository over the preview fakes: the account is `ciaran`, signed in
// through the keyring with a second login; codex-custom and arctic-explorer
// are cloned, openai/codex is PRs only, tomasz/bun-tools and ciaran/website
// ask for review without being added, and ~/Projects/website is a clone
// Pingex knows.

const KNOWN = [
  "/Users/ciaran/Projects/codex-custom",
  "/Users/ciaran/Projects/arctic-explorer",
  "/Users/ciaran/Projects/notes",
];

function setup(props: { initial?: AddRepositoryInitial } = {}) {
  const close = vi.fn();
  const addFolder = vi.fn(async () => {});
  render(AddRepositoryDialog, { knownFolders: KNOWN, addFolder, close, ...props });
  return { close, addFolder, user: userEvent.setup() };
}

const rowFor = (group: HTMLElement, slug: string) =>
  within(group).getByText(slug).closest("[data-repository]") as HTMLElement;

describe("AddRepositoryDialog", () => {
  beforeEach(() => {
    resetPreviewRepositories();
    resetRepositories();
  });

  it("lists repositories asking for my review and not yet added first, then mine", async () => {
    setup();

    const asking = await screen.findByRole("group", { name: "Asking for your review" });
    const slugs = within(asking)
      .getAllByRole("checkbox")
      .map((box) => box.closest("[data-repository]")?.getAttribute("data-repository"));
    expect(slugs).toEqual(["tomasz/bun-tools", "ciaran/website"]);
    expect(within(rowFor(asking, "tomasz/bun-tools")).getByText(/2 review requests · not cloned/)).toBeInTheDocument();
    expect(within(rowFor(asking, "ciaran/website")).getByText("Link clone")).toBeInTheDocument();
    expect(
      within(rowFor(asking, "ciaran/website")).getByText(/clone found at \/Users\/ciaran\/Projects\/website/),
    ).toBeInTheDocument();

    const yours = screen.getByRole("group", { name: "Your repositories" });
    expect(within(rowFor(yours, "ciaran/codex-custom")).getByText("Added")).toBeInTheDocument();
    expect(within(rowFor(yours, "ciaran/codex-custom")).queryByRole("checkbox")).not.toBeInTheDocument();
    expect(within(rowFor(yours, "ciaran/dotfiles")).getByRole("checkbox")).not.toBeChecked();
    // A repository already listed as asking for review is not listed twice.
    expect(within(yours).queryByText("ciaran/website")).not.toBeInTheDocument();
  });

  it("filters both lists by the search text", async () => {
    const { user } = setup();
    await screen.findByRole("group", { name: "Asking for your review" });

    await user.type(screen.getByRole("searchbox", { name: "Search repositories" }), "dot");

    expect(screen.queryByText("tomasz/bun-tools")).not.toBeInTheDocument();
    expect(screen.getByText("ciaran/dotfiles")).toBeInTheDocument();
  });

  it("shows the GitHub account and token source, and switches to another signed-in account", async () => {
    const { user } = setup();

    const account = await screen.findByTestId("github-account");
    await waitFor(() => expect(account).toHaveTextContent("GitHub: ciaran, using the token from gh (keyring)"));

    await user.click(within(account).getByRole("button", { name: "Change" }));
    await user.click(screen.getByRole("button", { name: "Use ciaran-work" }));

    await waitFor(() => expect(account).toHaveTextContent("GitHub: ciaran-work"));
    expect((await commands.reviewGithubAccount()).login).toBe("ciaran-work");
  });

  it("adds a review request as PRs only, clones one of mine and links a known clone", async () => {
    const { user, addFolder, close } = setup();
    const asking = await screen.findByRole("group", { name: "Asking for your review" });
    // Review requests start checked, PRs only.
    expect(within(rowFor(asking, "tomasz/bun-tools")).getByRole("checkbox")).toBeChecked();
    expect(within(rowFor(asking, "tomasz/bun-tools")).getByRole("radio", { name: "PRs only" })).toBeChecked();

    const yours = screen.getByRole("group", { name: "Your repositories" });
    const dotfiles = rowFor(yours, "ciaran/dotfiles");
    await user.click(within(dotfiles).getByRole("checkbox"));
    await user.click(within(dotfiles).getByRole("radio", { name: "Clone" }));
    await user.click(screen.getByRole("button", { name: "Add 3 repositories" }));

    await waitFor(() => expect(close).toHaveBeenCalledWith(true));
    const prsOnly = await commands.reviewPrsOnlyRepositories();
    expect(prsOnly.map((repo) => repo.slug).sort()).toEqual(["openai/codex", "tomasz/bun-tools"]);
    expect(addFolder).toHaveBeenCalledWith("/Users/ciaran/code/dotfiles", NATIVE_HOST);
    // A known clone is added where it is, its Host read from the path.
    expect(addFolder).toHaveBeenCalledWith("/Users/ciaran/Projects/website", null);
    const website = await commands.reviewRepository("/Users/ciaran/Projects/website");
    expect(website.state).toBe("cloned");
  });

  it("reads a local folder's remotes, defaults a fork to its upstream and stores the defaults", async () => {
    const { user, addFolder, close } = setup({ initial: { source: "folder" } });

    await user.type(screen.getByLabelText("Folder"), FORK_FOLDER);
    await user.tab();

    const facts = await screen.findByTestId("folder-facts");
    expect(facts).toHaveTextContent("Git repository");
    expect(facts).toHaveTextContent("on main");
    expect(facts).toHaveTextContent("2 linked worktrees");
    expect(facts).toHaveTextContent("bun.lock found");
    const sources = screen.getByRole("radiogroup", { name: "Pull requests from" });
    expect(within(sources).getByRole("radio", { name: /upstream · openai\/codex/ })).toBeChecked();
    expect(within(sources).getByText(/Only ones that ask for you or that you wrote/)).toBeInTheDocument();
    expect(screen.getByLabelText("Setup command")).toHaveValue("bun install");

    await user.click(screen.getByRole("radio", { name: "Create when needed" }));
    await user.click(screen.getByRole("button", { name: "Add repository" }));

    await waitFor(() => expect(close).toHaveBeenCalledWith(true));
    expect(addFolder).toHaveBeenCalledWith(FORK_FOLDER, NATIVE_HOST);
    const settings = await commands.reviewRepositorySettings(FORK_FOLDER);
    expect(settings).toMatchObject({
      prSource: "upstream",
      setupCommand: "bun install",
      reviewCheckoutPolicy: "whenNeeded",
    });
  });

  it("says when a folder is not a Git repository", async () => {
    const { user } = setup({ initial: { source: "folder" } });

    await user.type(screen.getByLabelText("Folder"), "/Users/ciaran/Downloads");
    await user.tab();

    expect(await screen.findByRole("alert")).toHaveTextContent("not a Git repository");
    expect(screen.getByRole("button", { name: "Add repository" })).toBeDisabled();
  });

  it("adds a repository by URL, PRs only or cloned", async () => {
    const { user, addFolder, close } = setup({ initial: { source: "url" } });

    await user.type(screen.getByLabelText("Repository URL"), "https://github.com/someone/tool.git");
    await user.click(screen.getByRole("button", { name: "Add repository" }));

    await waitFor(() => expect(close).toHaveBeenCalledWith(true));
    expect((await commands.reviewPrsOnlyRepositories()).map((repo) => repo.slug)).toContain("someone/tool");
    expect(addFolder).not.toHaveBeenCalled();
  });

  it("clones by URL into the chosen folder", async () => {
    const { user, addFolder } = setup({ initial: { source: "url", url: "someone/tool" } });

    await user.click(screen.getByRole("radio", { name: "Clone" }));
    const into = screen.getByLabelText("Clone into");
    await user.clear(into);
    await user.type(into, "/Users/ciaran/src");
    await user.click(screen.getByRole("button", { name: "Add repository" }));

    await waitFor(() => expect(addFolder).toHaveBeenCalledWith("/Users/ciaran/src/tool", NATIVE_HOST));
  });

  it("refuses text that is not a GitHub repository", async () => {
    const { user } = setup({ initial: { source: "url" } });

    await user.type(screen.getByLabelText("Repository URL"), "not a repo");

    expect(screen.getByRole("button", { name: "Add repository" })).toBeDisabled();
  });

  it("links an existing folder to a PRs-only repository, which it then replaces", async () => {
    const { user, addFolder } = setup({ initial: { source: "folder", linkSlug: "openai/codex" } });

    await user.type(screen.getByLabelText("Folder"), FORK_FOLDER);
    await user.tab();
    await screen.findByTestId("folder-facts");
    await user.click(screen.getByRole("button", { name: "Add repository" }));

    await waitFor(() => expect(addFolder).toHaveBeenCalledWith(FORK_FOLDER, NATIVE_HOST));
    expect((await commands.reviewPrsOnlyRepositories()).map((repo) => repo.slug)).not.toContain("openai/codex");
    const linked = await commands.reviewRepository(FORK_FOLDER);
    expect(linked.feeds.map((feed) => feed.slug)).toContain("openai/codex");
  });

  it("refuses to link a folder that does not clone the PRs-only repository", async () => {
    const { user, addFolder } = setup({ initial: { source: "folder", linkSlug: "openai/codex" } });

    await user.type(screen.getByLabelText("Folder"), "/Users/ciaran/Projects/website");
    await user.tab();
    await screen.findByTestId("folder-facts");
    await user.click(screen.getByRole("button", { name: "Add repository" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("no remote for openai/codex");
    expect(addFolder).not.toHaveBeenCalled();
  });
});
