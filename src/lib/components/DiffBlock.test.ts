import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import DiffBlock from "$lib/components/DiffBlock.svelte";
import type { FileUpdateChange } from "$lib/types";

const change = (path: string, diff: string): FileUpdateChange => ({
  path,
  diff,
  kind: { type: "update" },
});

function triggerFor(path: string): HTMLButtonElement {
  return screen.getByText(path).closest("button") as HTMLButtonElement;
}

describe("DiffBlock", () => {
  it("renders a small diff expanded and can collapse and reopen it", async () => {
    const user = userEvent.setup();
    render(DiffBlock, { change: change("small.ts", "@@ -1 +1 @@\n-old\n+new") });

    expect(screen.getByText("+new", { exact: false })).toBeVisible();
    await user.click(triggerFor("small.ts"));
    expect(screen.getByText("+new", { exact: false })).not.toBeVisible();
    await user.click(triggerFor("small.ts"));
    expect(screen.getByText("+new", { exact: false })).toBeVisible();
  });

  it("truncates an oversized diff until the user asks to show all", async () => {
    const user = userEvent.setup();
    const lines = Array.from({ length: 240 }, (_, index) => `+line-${index + 1}`);
    render(DiffBlock, { change: change("large.ts", lines.join("\n")) });

    expect(screen.getByText("+line-1", { exact: true })).not.toBeVisible();
    await user.click(triggerFor("large.ts"));
    expect(screen.getByText("+line-200", { exact: true })).toBeVisible();
    expect(screen.queryByText("+line-201", { exact: true })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Show all 240 lines" }));
    expect(screen.getByText("+line-240", { exact: true })).toBeVisible();
    expect(screen.queryByRole("button", { name: /Show all/ })).not.toBeInTheDocument();
  });

  it("starts collapsed when autoCollapse is set", async () => {
    const user = userEvent.setup();
    render(DiffBlock, { change: change("small.ts", "@@ -1 +1 @@\n-old\n+new"), autoCollapse: true });

    expect(screen.getByText("+new", { exact: false })).not.toBeVisible();
    await user.click(triggerFor("small.ts"));
    expect(screen.getByText("+new", { exact: false })).toBeVisible();
  });

  it("collapses when autoCollapse turns on unless the user already toggled it", async () => {
    const untouched = render(DiffBlock, {
      change: change("untouched.ts", "@@ -1 +1 @@\n-old\n+new"),
      autoCollapse: false,
    });
    expect(screen.getByText("+new", { exact: false })).toBeVisible();
    await untouched.rerender({ change: change("untouched.ts", "@@ -1 +1 @@\n-old\n+new"), autoCollapse: true });
    await waitFor(() => expect(screen.getByText("+new", { exact: false })).not.toBeVisible());
    untouched.unmount();

    const user = userEvent.setup();
    const toggled = render(DiffBlock, {
      change: change("toggled.ts", "@@ -1 +1 @@\n-old\n+kept"),
      autoCollapse: false,
    });
    await user.click(triggerFor("toggled.ts"));
    await user.click(triggerFor("toggled.ts"));
    await toggled.rerender({ change: change("toggled.ts", "@@ -1 +1 @@\n-old\n+kept"), autoCollapse: true });
    expect(screen.getByText("+kept", { exact: false })).toBeVisible();
  });
});
