import { beforeEach, describe, expect, it } from "vitest";
import {
  filterCounts,
  inboxSections,
  matchesText,
  NO_PEOPLE,
  personLabel,
  personOptions,
  primaryAction,
  prTimes,
  rangeSelection,
  reviewers,
  sectionOrder,
  stackPosition,
  syncDelay,
  UNASSIGNED,
  visibleRows,
} from "$lib/inbox/inbox";
import { SCENARIO_DIR, seedInboxScenario } from "$lib/inbox/inboxScenario";
import { commands } from "$lib/services/homeRouting";
import { inboxPr } from "$lib/services/preview/factories";
import type { GitHubPerson, Inbox, StackListing } from "$lib/types";

const MAIN = SCENARIO_DIR;
const VIEWER = "ciaran";
const NO_CHIPS = { drafts: false, ciFailing: false, unresolved: false };

let inbox: Inbox;
let stacks: StackListing;

beforeEach(async () => {
  seedInboxScenario();
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

  it("find a PR by an assignee's login or name", () => {
    expect(numbers(visibleRows(inbox, "everyone", NO_CHIPS, "tomasz"))).toEqual([420]);
    // 409 by its author's name, 415 and 420 by an assignee's.
    expect(numbers(visibleRows(inbox, "everyone", NO_CHIPS, "kovac"))).toEqual([409, 415, 420]);
  });
});

describe("the people filters", () => {
  const everyone = (people: { authors?: string[]; assignees?: string[] }) =>
    numbers(visibleRows(inbox, "everyone", NO_CHIPS, "", { ...NO_PEOPLE, ...people }));

  it("keep PRs by any chosen author and assigned to any chosen assignee", () => {
    expect(everyone({ authors: ["jo-r"] })).toEqual([401, 403]);
    expect(everyone({ authors: ["jo-r", "tomasz"] })).toEqual([401, 403, 420]);
    expect(everyone({ assignees: ["mira-k"] })).toEqual([415, 420]);
    expect(everyone({ authors: ["jo-r", "tomasz"], assignees: [VIEWER] })).toEqual([403]);
  });

  it("offer Unassigned for the PRs no one is assigned to", () => {
    const all = everyone({});
    expect(everyone({ assignees: [UNASSIGNED] })).toEqual(all.filter((n) => ![403, 415, 420].includes(n)));
    expect(everyone({ assignees: [UNASSIGNED, VIEWER] })).toEqual(all.filter((n) => ![415, 420].includes(n)));
  });

  it("narrow the tab counts, which read the backend's when no one is chosen", () => {
    expect(filterCounts(inbox, NO_PEOPLE)).toBe(inbox.counts);
    expect(filterCounts(inbox, { ...NO_PEOPLE, assignees: ["mira-k"] })).toEqual({
      needsMe: 1,
      mine: 1,
      reviewRequested: 0,
      everyone: 2,
    });
  });

  it("list me first, then by how many PRs each person has, counted without their own picker", () => {
    const options = personOptions(inbox, "everyone", NO_CHIPS, "", NO_PEOPLE);
    expect(options.assignees.map(({ login, count }) => [login, count])).toEqual([
      [UNASSIGNED, everyone({}).length - 3],
      [VIEWER, 1],
      ["mira-k", 2],
      ["tomasz", 1],
    ]);
    expect(options.authors[0]).toEqual({ login: VIEWER, count: inbox.counts.mine });

    const byJo = personOptions(inbox, "everyone", NO_CHIPS, "", { ...NO_PEOPLE, authors: ["jo-r"] });
    expect(byJo.authors).toEqual(options.authors);
    expect(byJo.assignees.map(({ login, count }) => [login, count])).toEqual([
      [UNASSIGNED, 1],
      [VIEWER, 1],
      ["mira-k", 0],
      ["tomasz", 0],
    ]);
  });

  it("keep a chosen person listed after their PRs have gone", () => {
    const options = personOptions(inbox, "everyone", NO_CHIPS, "", { ...NO_PEOPLE, authors: ["left-the-team"] });
    expect(options.authors).toContainEqual({ login: "left-the-team", count: 0 });
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

  it("show a whole stack, its other PRs dimmed, when asked to and one of its PRs matches", () => {
    const rows = visibleRows(inbox, "everyone", NO_CHIPS, "", { ...NO_PEOPLE, assignees: [VIEWER] });
    const theirStack = (wholeStacks: boolean) => {
      const items = inboxSections(inbox, stacks, rows, true, { wholeStacks }).flatMap((section) => section.items);
      expect(items).toHaveLength(1);
      if (items[0].kind !== "stack") throw new Error("expected jo-r's stack");
      return items[0].group;
    };
    const whole = theirStack(true);
    expect(whole.rows.map((row) => row.pr.number)).toEqual([401, 403]);
    expect([...whole.dimmed]).toEqual([401]);
    const narrowed = theirStack(false);
    expect(narrowed.rows.map((row) => row.pr.number)).toEqual([403]);
    expect(narrowed.dimmed.size).toBe(0);
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

describe("prTimes", () => {
  const now = Date.parse("2026-11-12T12:00:00Z");
  const times = (updatedAt: string) => prTimes({ createdAt: "2026-10-01T09:30:00Z", updatedAt }, now);

  it("says how long ago the PR last changed", () => {
    expect(times("2026-11-12T11:59:30Z").label).toBe("just now");
    expect(times("2026-11-12T11:15:00Z").label).toBe("45m ago");
    expect(times("2026-11-12T09:00:00Z").label).toBe("3h ago");
    expect(times("2026-11-05T12:00:00Z").label).toBe("7d ago");
  });

  it("gives the full dates for the hover", () => {
    const { opened, updated } = times("2026-11-05T12:00:00Z");
    expect(opened).toContain("2026");
    expect(updated).toContain("2026");
    expect(opened).not.toBe(updated);
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

describe("who is on a PR", () => {
  const person = (login: string, name: string | null = null): GitHubPerson => ({
    login,
    name,
    avatarUrl: null,
    isTeam: false,
  });

  it("says why someone is on the row, by name when GitHub has one", () => {
    expect(personLabel("mira-k", person("mira-k", "Mira Kovac"), "Approved")).toBe("Approved: Mira Kovac (mira-k)");
    expect(personLabel("tomasz", person("tomasz"), "Author")).toBe("Author: tomasz");
    expect(personLabel("tomasz")).toBe("tomasz");
  });

  it("gives each reviewer their role, and asking again replaces an old review", () => {
    const pr = inboxPr({
      number: 1,
      title: "Reviewed",
      reviews: [
        { author: "a", state: "APPROVED" },
        { author: "b", state: "CHANGES_REQUESTED" },
        { author: "c", state: "APPROVED" },
        { author: "d", state: "DISMISSED" },
      ],
      reviewRequests: ["c", "core-team"],
    });
    expect(reviewers(pr).map(({ login, role, tone, pending }) => [login, role, tone, pending])).toEqual([
      ["a", "Approved", "success", false],
      ["b", "Changes requested", "warning", false],
      ["c", "Review requested", "surface", true],
      ["d", "Reviewed", "surface", false],
      ["core-team", "Review requested", "surface", true],
    ]);
  });

  it("finds a PR by its author's display name", () => {
    const pr = inboxPr({ number: 1, author: "mk42", title: "Unrelated" });
    const people = new Map([["mk42", person("mk42", "Mira Kovac")]]);
    expect(matchesText(pr, "kovac", people)).toBe(true);
    expect(matchesText(pr, "kovac")).toBe(false);
  });
});
