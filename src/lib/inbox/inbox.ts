// The Pull requests inbox, as pure functions over a synced `Inbox`: the
// filters and chips, the sections and stack cards, each row's badges and
// primary action, multi-select ranges and the sync timer.
//
// Classification (Needs me, Mine, Review requested) is the backend's
// (`review/desk/sync.rs`), read from `inbox.triage`; nothing here
// re-derives it, so the filter counts and the sidebar badge agree.

import { ACTION_LABELS } from "$lib/review/actions";
import { stackName } from "$lib/stack/stack";
import type { FeedEntry, GitHubPerson, Inbox, InboxPr, PrStack, PrTriage, StackListing } from "$lib/types";

export type InboxFilter = "needsMe" | "mine" | "reviewRequested" | "everyone";

export const FILTER_LABELS: Record<InboxFilter, string> = {
  needsMe: "Needs me",
  mine: "Mine",
  reviewRequested: "Review requested",
  everyone: "Everyone",
};

/** The toggle chips: each one, when on, keeps only the PRs it names. */
export type InboxChips = { drafts: boolean; ciFailing: boolean; unresolved: boolean };

export type InboxRow = { pr: InboxPr; triage: PrTriage };

export function ciFailing(pr: InboxPr): boolean {
  return pr.checks === "FAILURE" || pr.checks === "ERROR";
}

function triageFor(inbox: Inbox, pr: InboxPr): PrTriage {
  return (
    inbox.triage.find((entry) => entry.number === pr.number) ?? {
      number: pr.number,
      mine: pr.author === inbox.viewer,
      reviewRequested: false,
      needsMe: null,
      myReview: null,
      commitsSinceMyReview: null,
      newActivity: 0,
      muted: false,
    }
  );
}

export function inboxRows(inbox: Inbox): InboxRow[] {
  return inbox.prs.map((pr) => ({ pr, triage: triageFor(inbox, pr) }));
}

function inFilter({ triage }: InboxRow, filter: InboxFilter): boolean {
  switch (filter) {
    case "needsMe":
      return triage.needsMe !== null;
    case "mine":
      return triage.mine;
    case "reviewRequested":
      return triage.reviewRequested;
    case "everyone":
      return true;
  }
}

function inChips({ pr }: InboxRow, chips: InboxChips): boolean {
  return (
    (!chips.drafts || pr.isDraft) &&
    (!chips.ciFailing || ciFailing(pr)) &&
    (!chips.unresolved || pr.unresolvedThreads > 0)
  );
}

/** Who each login in an inbox is. */
export type People = ReadonlyMap<string, GitHubPerson>;

export function peopleByLogin(inbox: Inbox): People {
  return new Map(inbox.people.map((person) => [person.login, person]));
}

/** Title, number (`#412` or `412`), author's login or name, or git branch, ignoring case. */
export function matchesText(pr: InboxPr, text: string, people?: People): boolean {
  const query = text.trim().toLowerCase();
  if (!query) return true;
  const number = query.replace(/^#/, "");
  if (/^\d+$/.test(number)) return String(pr.number).startsWith(number);
  const name = people?.get(pr.author)?.name ?? "";
  return [pr.title, pr.author, name, pr.headRef].some((field) => field.toLowerCase().includes(query));
}

export function visibleRows(inbox: Inbox, filter: InboxFilter, chips: InboxChips, text: string): InboxRow[] {
  const people = peopleByLogin(inbox);
  return inboxRows(inbox).filter(
    (row) => inFilter(row, filter) && inChips(row, chips) && matchesText(row.pr, text, people),
  );
}

// --- Sections and stacks ----------------------------------------------------

/** A stack's visible PRs, bottom first, with what its card shows. */
export type StackGroup = {
  stack: PrStack;
  /** The bottom PR's git branch, without an `owner/` prefix. */
  name: string;
  author: string;
  rows: InboxRow[];
};

export type InboxItem = { kind: "stack"; group: StackGroup } | { kind: "pr"; row: InboxRow };

export type InboxSection = {
  id: "yourStacks" | "yours" | "requested" | "others";
  title: string;
  items: InboxItem[];
};

/** The open stacks whose PRs are all by one author, with their visible rows. */
function stackGroups(inbox: Inbox, listing: StackListing, rows: InboxRow[]): StackGroup[] {
  if (!listing.supported) return [];
  const byNumber = new Map(rows.map((row) => [row.pr.number, row]));
  const authorOf = new Map(inbox.prs.map((pr) => [pr.number, pr.author]));
  const groups: StackGroup[] = [];
  for (const stack of listing.stacks) {
    if (!stack.open) continue;
    const open = stack.prs.filter((entry) => entry.state === "open");
    const authors = new Set(open.map((entry) => authorOf.get(entry.number)));
    const author = [...authors][0];
    if (authors.size !== 1 || !author) continue;
    const visible = open.flatMap((entry) => byNumber.get(entry.number) ?? []);
    if (visible.length > 0) groups.push({ stack, name: stackName(stack), author, rows: visible });
  }
  return groups;
}

/**
 * Where a PR sits in its stack: 1 is the PR on the base, counted among the
 * stack's open PRs. A PR keeps its number when a filter hides the PRs below it.
 */
export function stackPosition(group: StackGroup, number: number): { position: number; total: number } {
  const open = group.stack.prs.filter((entry) => entry.state === "open");
  return { position: open.findIndex((entry) => entry.number === number) + 1, total: open.length };
}

/**
 * The inbox's sections, in order: my stacks as cards, my other PRs, review
 * requests (grouped by the author's stack), then everyone else. Empty
 * sections are left out. With `groupByStack` off, or no native stacks, every
 * PR is its own row.
 */
export function inboxSections(
  inbox: Inbox,
  listing: StackListing,
  rows: InboxRow[],
  groupByStack: boolean,
): InboxSection[] {
  const groups = groupByStack ? stackGroups(inbox, listing, rows) : [];
  const stacked = new Map<number, StackGroup>();
  for (const group of groups) for (const row of group.rows) stacked.set(row.pr.number, group);

  const yourStacks: InboxItem[] = groups
    .filter((group) => group.author === inbox.viewer)
    .map((group) => ({ kind: "stack", group }));
  const sections: InboxSection[] = [
    { id: "yourStacks", title: "Your stacks", items: yourStacks },
    { id: "yours", title: "Your pull requests", items: [] },
    { id: "requested", title: "Review requested", items: [] },
    { id: "others", title: "Everyone else", items: [] },
  ];
  const placed = new Set<StackGroup>();
  for (const row of rows) {
    const id = row.triage.mine ? "yours" : row.triage.reviewRequested ? "requested" : "others";
    const group = stacked.get(row.pr.number);
    if (group?.author === inbox.viewer) continue;
    const section = sections.find((candidate) => candidate.id === id) as InboxSection;
    if (!group) {
      section.items.push({ kind: "pr", row });
    } else if (!placed.has(group)) {
      placed.add(group);
      section.items.push({ kind: "stack", group });
    }
  }
  return sections.filter((section) => section.items.length > 0);
}

/** PR numbers in display order, for ⇧-click ranges. */
export function sectionOrder(sections: InboxSection[]): number[] {
  return sections.flatMap((section) =>
    section.items.flatMap((item) =>
      item.kind === "pr" ? [item.row.pr.number] : item.group.rows.map((row) => row.pr.number),
    ),
  );
}

// --- Rows -------------------------------------------------------------------

export type BadgeTone = "success" | "warning" | "error" | "primary" | "surface";

export type Badge = { label: string; tone: BadgeTone };

export function reviewBadge(pr: InboxPr): Badge | null {
  if (pr.isDraft) return { label: "Draft", tone: "surface" };
  switch (pr.reviewDecision) {
    case "APPROVED":
      return { label: "Approved", tone: "success" };
    case "CHANGES_REQUESTED":
      return { label: "Changes requested", tone: "warning" };
    case "REVIEW_REQUIRED":
      return { label: "Review pending", tone: "surface" };
    default:
      return null;
  }
}

export function ciBadge(pr: InboxPr): Badge | null {
  if (ciFailing(pr)) return { label: "CI failing", tone: "error" };
  if (pr.checks === "SUCCESS") return { label: "CI passing", tone: "success" };
  if (pr.checks === "PENDING" || pr.checks === "EXPECTED") return { label: "CI running", tone: "surface" };
  return null;
}

/** The state ring before a row: approved, changes requested, failing, draft. */
export function stateTone(pr: InboxPr): BadgeTone {
  if (ciFailing(pr)) return "error";
  if (pr.reviewDecision === "CHANGES_REQUESTED") return "warning";
  if (pr.reviewDecision === "APPROVED") return "success";
  return "surface";
}

/** Someone on a PR other than its author. */
export type Reviewer = { login: string; role: string; tone: BadgeTone; pending: boolean };

/** People on the PR: who reviewed, then who is asked to. Asked again after
 *  reviewing, their old review no longer counts. */
export function reviewers(pr: InboxPr): Reviewer[] {
  const logins = [...new Set([...pr.reviews.map((review) => review.author), ...pr.reviewRequests])];
  return logins.map((login): Reviewer => {
    const state = pr.reviewRequests.includes(login)
      ? null
      : pr.reviews.find((review) => review.author === login)?.state;
    switch (state) {
      case null:
        return { login, role: "Review requested", tone: "surface", pending: true };
      case "APPROVED":
        return { login, role: "Approved", tone: "success", pending: false };
      case "CHANGES_REQUESTED":
        return { login, role: "Changes requested", tone: "warning", pending: false };
      default:
        return { login, role: "Reviewed", tone: "surface", pending: false };
    }
  });
}

/** What hovering someone's avatar says: "Approved: Mira Kovac (mira-k)". */
export function personLabel(login: string, person?: GitHubPerson | null, role?: string | null): string {
  const who = person?.name ? `${person.name} (${login})` : login;
  return role ? `${role}: ${who}` : who;
}

export function initials(login: string): string {
  const parts = login.split(/[-_.\s]+/).filter(Boolean);
  const letters = parts.length > 1 ? parts[0][0] + parts[1][0] : login.slice(0, 2);
  return letters.toUpperCase();
}

/** The PR's latest agent review, as the row shows it: in flight, or
 *  finished ("Agent review ready · N findings"). */
export type AgentReviewState = "queued" | "running" | "ready";

export type PrimaryActionId =
  | "merge"
  | "respond"
  | "fixCi"
  | "agentReview"
  | "review"
  | "changesSince"
  | "reviewStack"
  | "startReview"
  | "open";

export type PrimaryAction = { id: PrimaryActionId; label: string };

/**
 * The one action a row offers, from its state. `stackAbove` is set for a
 * review request that sits above another PR in the author's stack.
 */
export function primaryAction(
  { pr, triage }: InboxRow,
  context: { stackAbove?: boolean; agentReview?: AgentReviewState | null } = {},
): PrimaryAction {
  const action = (id: PrimaryActionId, label: string): PrimaryAction => ({ id, label });
  if (triage.mine) {
    switch (triage.needsMe) {
      case "readyToMerge":
        return action("merge", ACTION_LABELS.merge);
      case "changesRequested":
      case "unresolved":
        return action("respond", "Respond");
      case "checksFailing":
        return action("fixCi", ACTION_LABELS.fixCi);
    }
    if (pr.reviews.length === 0 && !context.agentReview) return action("agentReview", "Agent review");
    return action("open", "Open");
  }
  if ((triage.commitsSinceMyReview ?? 0) > 0) return action("changesSince", ACTION_LABELS.changesSince);
  if (triage.reviewRequested) {
    if (context.stackAbove) return action("reviewStack", "Review stack");
    if (context.agentReview) return action("review", "Review");
    return action("startReview", "Start review");
  }
  return action("open", "Open");
}

// --- Feed -------------------------------------------------------------------

const REVIEW_VERBS: Record<string, string> = {
  APPROVED: "approved",
  CHANGES_REQUESTED: "requested changes on",
  COMMENTED: "commented on",
  DISMISSED: "dismissed a review on",
};

/** One feed line: who did what to which PR. */
export function feedText(entry: FeedEntry): { actor: string | null; text: string } {
  const pr = `#${entry.number}`;
  switch (entry.kind) {
    case "reviewed":
      return { actor: entry.actor, text: `${REVIEW_VERBS[entry.state ?? ""] ?? "reviewed"} ${pr}` };
    case "pushed":
      return { actor: entry.actor, text: `pushed to ${pr}` };
    case "opened":
      return { actor: entry.actor, text: `opened ${pr}` };
    case "reviewRequested":
      return { actor: null, text: `Your review was requested on ${pr}` };
    case "checksFailed":
      return { actor: null, text: `CI failed on ${pr}` };
    case "checksPassed":
      return { actor: null, text: `CI passed on ${pr}` };
    case "closed":
      return { actor: null, text: `${pr} was closed` };
    default:
      return { actor: entry.actor, text: `${entry.kind} ${pr}` };
  }
}

/** A click on a row's own control, which must not also open the row. */
export function isRowControl(target: EventTarget | null): boolean {
  return target instanceof Element && target.closest("button, a, input, [role=menuitem]") !== null;
}

// --- Selection and sync -----------------------------------------------------

/** `selected` plus every row from `anchor` to `target` in `order`. */
export function rangeSelection(order: number[], anchor: number, target: number, selected: Set<number>): Set<number> {
  const [from, to] = [order.indexOf(anchor), order.indexOf(target)];
  const next = new Set(selected);
  if (from < 0 || to < 0) return next.add(target);
  for (const number of order.slice(Math.min(from, to), Math.max(from, to) + 1)) next.add(number);
  return next;
}

const SYNC_EVERY_MS = 120_000;

/** How long the focused window waits before the next timed sync, given the
 *  GraphQL points left: two minutes, backing off as they run low. */
export function syncDelay(remaining: number | null): number {
  if (remaining === null || remaining >= 1000) return SYNC_EVERY_MS;
  if (remaining >= 200) return SYNC_EVERY_MS * 5;
  return SYNC_EVERY_MS * 15;
}
