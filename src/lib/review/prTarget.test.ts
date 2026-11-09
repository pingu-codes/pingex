import { describe, expect, it } from "vitest";
import type { StackListing } from "$lib/types";
import { stackOfPr } from "./prTarget";

const entry = (number: number, headRef: string, state = "open", merged = false) => ({
  number,
  state,
  isDraft: false,
  merged,
  headRef,
  headSha: "a".repeat(40),
});

const listing: StackListing = {
  supported: true,
  stacks: [
    {
      number: 7,
      baseRef: "main",
      open: true,
      prs: [entry(1, "owner/feat-a", "merged", true), entry(2, "feat-b"), entry(3, "feat-c")],
    },
  ],
};

describe("stackOfPr", () => {
  it("names the stack and lists its open branches, bottom first", () => {
    expect(stackOfPr(listing, 3)).toEqual({ name: "feat-a", branches: ["feat-b", "feat-c"] });
  });

  it("finds no stack for a PR outside any", () => {
    expect(stackOfPr(listing, 99)).toBeNull();
  });

  it("finds no stack for a merged member", () => {
    expect(stackOfPr(listing, 1)).toBeNull();
  });

  it("finds no stack where GitHub offers none or the inbox hasn't synced", () => {
    expect(stackOfPr({ ...listing, supported: false }, 2)).toBeNull();
    expect(stackOfPr(null, 2)).toBeNull();
  });
});
