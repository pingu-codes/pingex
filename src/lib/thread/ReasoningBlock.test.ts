import { render, screen } from "@testing-library/svelte";
import { describe, expect, it } from "vitest";
import ReasoningBlock from "$lib/thread/ReasoningBlock.svelte";
import type { ThreadItem } from "$lib/types";

const items: ThreadItem[] = [
  { id: "r1", type: "reasoning", summary: ["First **thought**"] },
  { id: "r2", type: "reasoning", summary: ["Latest thought"] },
];

describe("ReasoningBlock", () => {
  it("shows the latest summary while reasoning is live", () => {
    render(ReasoningBlock, { items, live: true });

    expect(screen.getByText("Working…")).toBeVisible();
    expect(screen.getByText("Latest thought")).toBeVisible();
    expect(screen.queryByText("First thought")).not.toBeInTheDocument();
  });

  it("shows all summaries after reasoning settles", () => {
    render(ReasoningBlock, { items });

    expect(screen.queryByText("Working…")).not.toBeInTheDocument();
    expect(screen.getByText("thought", { selector: "strong" })).toBeVisible();
    expect(screen.getByText("Latest thought")).toBeVisible();
  });

  it("renders no status for empty settled reasoning", () => {
    const { container } = render(ReasoningBlock, { items: [] });
    expect(container).toHaveTextContent("");
  });
});
