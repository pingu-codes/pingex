import { describe, expect, it } from "vitest";
import { fileIconFor, fileIconSvg, folderIcon, iconForPath } from "$lib/utils/fileIcons";

describe("fileIconFor", () => {
  it("gives related extensions the same glyph but their own tint", () => {
    expect(fileIconFor("utils.ts").class).toBe("text-blue-500");
    expect(fileIconFor("alias.config.js").class).toBe("text-yellow-500");
    expect(fileIconFor("utils.ts").body).toBe(fileIconFor("alias.config.js").body);
  });

  it("distinguishes glyph families, not just colours", () => {
    const bodies = ["a.ts", "a.json", "a.sql", "a.sh", "a.png", "a.md", "deno.lock"].map(
      (name) => fileIconFor(name).body,
    );
    expect(new Set(bodies).size).toBe(bodies.length);
  });

  it("is case-insensitive and keys off the final extension", () => {
    expect(fileIconFor("README.MD")).toEqual(fileIconFor("notes.md"));
    expect(fileIconFor("vite.config.TS")).toEqual(fileIconFor("a.ts"));
  });

  it("falls back to a generic file for unknown or extensionless names", () => {
    expect(fileIconFor("Makefile").class).toBe("text-surface-500");
    expect(fileIconFor("data.xyz")).toEqual(fileIconFor("Makefile"));
  });
});

describe("iconForPath", () => {
  it("treats a trailing slash as a directory", () => {
    expect(iconForPath("lib", "src/lib/")).toEqual(folderIcon);
    expect(iconForPath("lib", "/proj/src/lib/")).toEqual(folderIcon);
  });

  it("treats an extensionless file as a file, not a folder", () => {
    expect(iconForPath("LICENSE", "LICENSE")).not.toEqual(folderIcon);
  });
});

describe("fileIconSvg", () => {
  it("renders standalone markup that inherits colour from its container", () => {
    const svg = fileIconSvg(folderIcon, "size-3.5 shrink-0");
    expect(svg).toContain('class="size-3.5 shrink-0"');
    expect(svg).toContain('stroke="currentColor"');
    expect(svg).toContain(folderIcon.body);
  });

  it("produces markup the DOM parses into a single svg element", () => {
    const host = document.createElement("span");
    host.innerHTML = fileIconSvg(fileIconFor("a.ts"), "size-3.5");
    expect(host.children).toHaveLength(1);
    expect(host.firstElementChild?.tagName.toLowerCase()).toBe("svg");
    expect(host.querySelectorAll("path").length).toBeGreaterThan(1);
  });
});
