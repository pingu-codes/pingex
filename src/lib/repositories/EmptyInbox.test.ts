import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import EmptyInbox from "$lib/repositories/EmptyInbox.svelte";
import { resetPreviewRepositories, setPreviewGithubAccount } from "$lib/services/preview/repositories";

// The inbox with no repositories: Add repository, and connecting GitHub when
// no account is signed in.
describe("EmptyInbox", () => {
  afterEach(() => resetPreviewRepositories());

  it("offers Add repository", async () => {
    const onAddRepository = vi.fn();
    render(EmptyInbox, { onAddRepository });

    expect(screen.getByText("No repositories yet. Add one to see its pull requests here.")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "Add repository" }));
    expect(onAddRepository).toHaveBeenCalledOnce();
  });

  it("says how to connect GitHub when no account is signed in", async () => {
    setPreviewGithubAccount({ login: null, tokenSource: null, logins: [] });
    render(EmptyInbox, { onAddRepository: vi.fn() });

    const connect = await screen.findByTestId("connect-github");
    expect(connect).toHaveTextContent("Or connect GitHub to list the ones asking for your review");
    expect(connect).toHaveTextContent("gh auth login");
  });

  it("names the account when GitHub is already connected", async () => {
    render(EmptyInbox, { onAddRepository: vi.fn() });

    expect(await screen.findByText(/Signed in to GitHub as ciaran/)).toBeInTheDocument();
    expect(screen.queryByTestId("connect-github")).not.toBeInTheDocument();
  });
});
