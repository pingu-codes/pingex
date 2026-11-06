import { beforeEach, describe, expect, it } from "vitest";
import {
  inboxSections,
  primaryAction,
  rangeSelection,
  sectionOrder,
  stackPosition,
  syncDelay,
  visibleRows,
} from "$lib/inbox/inbox";
import { commands } from "$lib/services/homeRouting";
import { resetDeskActivity } from "$lib/services/preview/desk";
import type { Inbox, StackListing } from "$lib/types";

const MAIN = "/Users/ciaran/Projects/codex-custom";
const NO_CHIPS = { drafts: false, ciFailing: false, unresolved: false };

let inbox: Inbox;
let stacks: StackListing;

beforeEach(async () => {
  resetDeskActivity();
  inbox = await commands.reviewInbox(MAIN);
  stacks = await commands.reviewStacks(MAIN);
});

const numbers = (rows: { pr: { number: number } }[]) => rows.map((row) => row.pr.number).sort((a, b) => a - b);

describe("the inbox filters", () => {
  it("narrow by the backend's classification, then by chips and text", () => {
    expect(numbers(visibleRows(inbox, "mine", NO_CHIPS, ""))).toEqual([128, 412, 415, 418]);
    expect(numbers(visibleRows(inbox, "everyone", { ...NO_CHIPS, drafts: true }, ""))).toEqual([126, 418]);
    expect(numbers(visibleRows(inbox, "everyone", { ...NO_CHIPS, ciFailing: true }, ""))).toEqual([418]);
    expect(numbers(visibleRows(inbox, "mine", { ...NO_CHIPS, unresolved: true }, ""))).toEqual([128, 415]);
    expect(numbers(visibleRows(inbox, "everyone", NO_CHIPS, "#420"))).toEqual([420]);
    expect(numbers(visibleRows(inbox, "everyone", NO_CHIPS, "jo-r"))).toEqual([401, 403]);
    expect(numbers(visibleRows(inbox, "everyone", NO_CHIPS, "maki-acp"))).toEqual([418]);
    expect(numbers(visibleRows(inbox, "everyone", NO_CHIPS, "HANDSHAKE"))).toEqual([409]);
  });
});

describe("the inbox sections", () => {
  it("show my stacks as cards and group review requests by the author's stack", () => {
    const sections = inboxSections(inbox, stacks, visibleRows(inbox, "everyone", NO_CHIPS, ""), true);
    expect(sections.map((section) => section.title)).toEqual([
      "Your stacks",
      "Your pull requests",
      "Review requested",
      "Everyone else",
    ]);
    const [yours, mine, requested] = sections;
    expect(yours.items).toHaveLength(1);
    const card = yours.items[0];
    expect(card.kind === "stack" && card.group.rows.map((row) => row.pr.number)).toEqual([412, 415, 418]);
    expect(mine.items.map((item) => item.kind === "pr" && item.row.pr.number)).toEqual([128]);
    const group = requested.items.find((item) => item.kind === "stack");
    expect(group?.kind === "stack" && [group.group.author, group.group.rows.map((row) => row.pr.number)]).toEqual([
      "jo-r",
      [401, 403],
    ]);
    expect(sectionOrder(sections).slice(0, 4)).toEqual([412, 415, 418, 128]);
  });

  it("list every PR on its own with Group by stack off, or when the repository has no stacks", () => {
    const rows = visibleRows(inbox, "everyone", NO_CHIPS, "");
    for (const sections of [
      inboxSections(inbox, stacks, rows, false),
      inboxSections(inbox, { supported: false, stacks: [] }, rows, true),
    ]) {
      expect(sections.some((section) => section.items.some((item) => item.kind === "stack"))).toBe(false);
      expect(sectionOrder(sections)).toHaveLength(rows.length);
    }
  });
});

describe("a PR's place in its stack", () => {
  const myStack = (rows: ReturnType<typeof visibleRows>) => {
    const card = inboxSections(inbox, stacks, rows, true)[0].items[0];
    if (card.kind !== "stack") throw new Error("expected my stack");
    return card.group;
  };

  it("counts from the PR on the base, among the open PRs", () => {
    const group = myStack(visibleRows(inbox, "everyone", NO_CHIPS, ""));
    expect(group.rows.map((row) => stackPosition(group, row.pr.number))).toEqual([
      { position: 1, total: 3 },
      { position: 2, total: 3 },
      { position: 3, total: 3 },
    ]);
    const merged = { ...group.stack.prs[0], number: 400, state: "closed", merged: true };
    const withMerged = { ...group, stack: { ...group.stack, prs: [merged, ...group.stack.prs] } };
    expect(stackPosition(withMerged, 412)).toEqual({ position: 1, total: 3 });
  });

  it("keeps a PR's number when a filter hides the PRs below it", () => {
    const group = myStack(visibleRows(inbox, "everyone", { ...NO_CHIPS, ciFailing: true }, ""));
    expect(group.rows.map((row) => row.pr.number)).toEqual([418]);
    expect(stackPosition(group, 418)).toEqual({ position: 3, total: 3 });
  });
});

describe("the primary action", () => {
  const row = (number: number) => {
    const rows = visibleRows(inbox, "everyone", NO_CHIPS, "");
    return rows.find((candidate) => candidate.pr.number === number)!;
  };

  it("fits the PR's state", () => {
    expect(primaryAction(row(412)).label).toBe("Merge");
    expect(primaryAction(row(415)).label).toBe("Respond");
    expect(primaryAction(row(418)).label).toBe("Fix CI in thread");
    expect(primaryAction(row(401)).label).toBe("Review changes since");
    expect(primaryAction(row(403), { stackAbove: true }).label).toBe("Review stack");
    expect(primaryAction(row(409)).label).toBe("Start review");
    expect(primaryAction(row(409), { agentReview: "running" }).label).toBe("Review");
    expect(primaryAction(row(420)).label).toBe("Open");
  });
});

describe("selection", () => {
  it("selects a range between the anchor and the clicked row, in display order", () => {
    const order = [412, 415, 418, 128, 401];
    expect([...rangeSelection(order, 415, 128, new Set([401]))].sort()).toEqual([128, 401, 415, 418]);
    expect([...rangeSelection(order, 128, 412, new Set())].sort()).toEqual([128, 412, 415, 418]);
  });
});

describe("the sync timer", () => {
  it("waits two minutes, and backs off as the rate limit runs low", () => {
    expect(syncDelay(null)).toBe(120_000);
    expect(syncDelay(4000)).toBe(120_000);
    expect(syncDelay(800)).toBeGreaterThan(120_000);
    expect(syncDelay(50)).toBeGreaterThan(syncDelay(800));
  });
});
