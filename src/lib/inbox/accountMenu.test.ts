import { describe, expect, it } from "vitest";
import { accountEntries, accountReason, viewerMismatch } from "$lib/inbox/accountMenu";
import type { RepositoryAccount } from "$lib/types";

const account = (overrides: Partial<RepositoryAccount> = {}): RepositoryAccount => ({
  login: "work",
  source: "global",
  pin: null,
  logins: ["personal", "work"],
  global: "work",
  ...overrides,
});

const items = (entries: ReturnType<typeof accountEntries>) =>
  entries.flatMap((entry) => (entry.kind === "separator" ? [] : [entry]));

describe("the account menu", () => {
  it("offers automatic, each account for this repository, and the global account", () => {
    const entries = items(accountEntries(account()));
    expect(entries.map((entry) => entry.id)).toEqual(["auto", "pin:personal", "pin:work", "global"]);
    expect(entries[0]).toMatchObject({ checked: true });
    const global = entries[3];
    expect(global.kind === "submenu" && global.entries).toMatchObject([
      { id: "global:personal", checked: false },
      { id: "global:work", checked: true },
    ]);
  });

  it("ticks the pinned account rather than automatic", () => {
    const entries = items(accountEntries(account({ source: "pinned", pin: "personal", login: "personal" })));
    expect(entries[0]).toMatchObject({ checked: false });
    expect(entries[1]).toMatchObject({ id: "pin:personal", checked: true });
  });

  it("has no global choice with one account signed in", () => {
    const entries = items(accountEntries(account({ logins: ["work"] })));
    expect(entries.map((entry) => entry.id)).toEqual(["auto", "pin:work"]);
  });

  it("shows a pin whose account has gone, and automatic still undoes it", () => {
    const pinned = account({ source: "pinMissing", pin: "gone", login: "gone" });
    const entries = items(accountEntries(pinned));
    expect(entries.find((entry) => entry.id === "missing")).toMatchObject({ disabled: true, checked: true });
    expect(entries[0]).toMatchObject({ id: "auto", checked: false });
    expect(accountReason(pinned)).toContain("gone is not signed in");
  });

  it("goes read-only when a token in the environment decides", () => {
    const entries = items(accountEntries(account({ source: "environment" })));
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ disabled: true });
  });
});

describe("a viewer that disagrees with the account", () => {
  it("is flagged", () => {
    expect(viewerMismatch(account({ login: "personal", source: "owner" }), "work")).toBe(true);
  });

  it("is not flagged for a match in any case, before a sync, or for a pin that has gone", () => {
    expect(viewerMismatch(account({ login: "Work" }), "work")).toBe(false);
    expect(viewerMismatch(account(), null)).toBe(false);
    expect(viewerMismatch(account({ source: "pinMissing", login: "gone" }), "work")).toBe(false);
  });
});
