import { describe, expect, it } from "vitest";
import {
  countLabel,
  emptyGroup,
  filterChipLabel,
  hasMore,
  noMatchLabel,
  type SearchGroup,
  searchState,
} from "$lib/layout/sidebarSearch";
import type { ThreadSearchItem } from "$lib/types";

function item(id: string): ThreadSearchItem {
  return { id, title: id, preview: id, cwd: "/proj", updatedAt: 0, archived: false };
}

function group(overrides: Partial<SearchGroup> = {}): SearchGroup {
  return { ...emptyGroup(), ...overrides };
}

describe("searchState", () => {
  const base = { active: emptyGroup(), archived: emptyGroup(), error: null, loading: false };

  it("is idle before a query is entered", () => {
    expect(searchState({ ...base, query: "  " })).toBe("idle");
  });

  it("is loading while the first page is in flight", () => {
    expect(searchState({ ...base, query: "log", loading: true })).toBe("loading");
  });

  it("stays on results while loading more once something is shown", () => {
    const active = group({ items: [item("a")], total: 5 });
    expect(searchState({ ...base, active, query: "log", loading: true })).toBe("results");
  });

  it("is empty when a finished query matched nothing", () => {
    expect(searchState({ ...base, query: "zzz" })).toBe("empty");
  });

  it("is results when either group has matches", () => {
    const archived = group({ items: [item("a")], total: 1 });
    expect(searchState({ ...base, archived, query: "log" })).toBe("results");
  });

  it("surfaces errors above all else", () => {
    expect(searchState({ ...base, query: "log", error: new Error("db") })).toBe("error");
  });
});

describe("labels", () => {
  it("counts loaded of total", () => {
    expect(countLabel(0, 0)).toBe("0");
    expect(countLabel(12, 40)).toBe("12 of 40");
    expect(countLabel(40, 40)).toBe("40");
    expect(countLabel(45, 40)).toBe("40");
  });

  it("quotes the query in the no-match label", () => {
    expect(noMatchLabel("  auth  ")).toBe('No results for "auth"');
  });

  it("only shows a filter chip when scoped to a project", () => {
    expect(filterChipLabel(null)).toBeNull();
    expect(filterChipLabel("codex-custom")).toBe("codex-custom");
  });
});

describe("hasMore", () => {
  it("is true only when a cursor remains", () => {
    expect(hasMore(group({ cursor: "20" }))).toBe(true);
    expect(hasMore(group({ cursor: null }))).toBe(false);
  });
});
