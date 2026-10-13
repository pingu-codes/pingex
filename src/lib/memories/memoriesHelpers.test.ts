// @vitest-environment node
import { describe, expect, it } from "vitest";
import {
  defaultEntry,
  entryCount,
  filterMemories,
  findEntry,
  formatAge,
  isMarkdown,
  stripFrontmatter,
} from "$lib/memories/memoriesHelpers";
import type { MemoryEntry, MemoryHome } from "$lib/types";

function entry(relPath: string, patch: Partial<MemoryEntry> = {}): MemoryEntry {
  return {
    path: `/cfg/memory/${relPath}`,
    relPath,
    title: relPath.replace(/\.md$/, ""),
    description: null,
    kind: null,
    modifiedAt: null,
    bytes: 10,
    unindexed: false,
    ...patch,
  };
}

const homes: MemoryHome[] = [
  {
    homeId: "claude",
    label: "Claude Code",
    configDir: "/cfg",
    toggles: [],
    error: null,
    stores: [
      {
        root: "/cfg/memory",
        title: "codex-custom",
        projectPath: "/work/codex-custom",
        entries: [
          entry("MEMORY.md", { kind: "index" }),
          entry("roadmap.md", { description: "Feature roadmap status", kind: "project" }),
          entry("penpot.md", { description: "Penpot MCP quirks", kind: "reference" }),
        ],
      },
      { root: "/cfg/other", title: "other", projectPath: null, entries: [] },
    ],
  },
];

describe("filterMemories", () => {
  it("keeps every store for an empty query", () => {
    const [home] = filterMemories(homes, "  ");
    expect(home.stores.map((store) => store.entries.length)).toEqual([3, 0]);
  });

  it("matches every word against entry text and drops empty stores", () => {
    const [home] = filterMemories(homes, "penpot quirks");
    expect(home.stores).toHaveLength(1);
    expect(home.stores[0].entries.map((e) => e.relPath)).toEqual(["penpot.md"]);
    expect(home.stores[0].total).toBe(3);
  });

  it("keeps a whole store when its project matches", () => {
    const [home] = filterMemories(homes, "codex-custom");
    expect(home.stores[0].entries).toHaveLength(3);
  });

  it("matches kinds", () => {
    expect(filterMemories(homes, "reference")[0].stores[0].entries.map((e) => e.relPath)).toEqual(["penpot.md"]);
  });
});

describe("entry lookup", () => {
  it("finds an entry by home and path", () => {
    expect(findEntry(homes, "claude", "/cfg/memory/roadmap.md")?.entry.title).toBe("roadmap");
    expect(findEntry(homes, "codex", "/cfg/memory/roadmap.md")).toBeNull();
    expect(findEntry(homes, null, null)).toBeNull();
  });

  it("opens the index first", () => {
    expect(defaultEntry(homes)).toEqual({ homeId: "claude", path: "/cfg/memory/MEMORY.md" });
    expect(defaultEntry([])).toBeNull();
    expect(entryCount(homes)).toBe(3);
  });
});

describe("stripFrontmatter", () => {
  it("drops a leading frontmatter block only", () => {
    expect(stripFrontmatter("---\nname: x\ntype: user\n---\n\nBody")).toBe("\nBody");
    expect(stripFrontmatter("Intro\n---\nname: x\n---\n")).toBe("Intro\n---\nname: x\n---\n");
    expect(stripFrontmatter("---\nname: x\n---")).toBe("");
  });
});

describe("formatting", () => {
  it("formats ages and file types", () => {
    const now = 1_000_000_000_000;
    expect(formatAge(now / 1000 - 30, now)).toBe("just now");
    expect(formatAge(now / 1000 - 7200, now)).toBe("2h ago");
    expect(formatAge(null, now)).toBe("");
    expect(isMarkdown("skills/x/SKILL.md")).toBe(true);
    expect(isMarkdown("skills/x/run.sh")).toBe(false);
  });
});
