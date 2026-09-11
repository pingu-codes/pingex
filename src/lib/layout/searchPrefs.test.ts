import { beforeEach, describe, expect, it } from "vitest";
import {
  loadSearchAgentMessages,
  loadSearchUserMessages,
  searchPrefs,
} from "$lib/layout/searchPrefs.svelte";

beforeEach(() => localStorage.clear());

describe("search prefs", () => {
  it("defaults both message scopes to off", () => {
    expect(loadSearchAgentMessages()).toBe(false);
    expect(loadSearchUserMessages()).toBe(false);
  });

  it("persists each scope independently", () => {
    searchPrefs.setSearchAgentMessages(true);
    expect(loadSearchAgentMessages()).toBe(true);
    expect(loadSearchUserMessages()).toBe(false);
    expect(searchPrefs.searchAgentMessages).toBe(true);

    searchPrefs.setSearchUserMessages(true);
    searchPrefs.setSearchAgentMessages(false);
    expect(loadSearchAgentMessages()).toBe(false);
    expect(loadSearchUserMessages()).toBe(true);
  });

  it("treats anything but the literal true as off", () => {
    localStorage.setItem("pingex-search-user-messages", "yes");
    expect(loadSearchUserMessages()).toBe(false);
  });
});
