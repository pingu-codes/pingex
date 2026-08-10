import { describe, expect, it } from "vitest";
import { detectSlashQuery, filterSlashCommands, parseSlashCommand, SLASH_COMMANDS } from "$lib/composer/slashCommands";

describe("detectSlashQuery", () => {
  it("detects a bare slash as an empty query", () => {
    expect(detectSlashQuery("/")).toBe("");
  });

  it("detects a partial command", () => {
    expect(detectSlashQuery("/pla")).toBe("pla");
  });

  it("ignores a slash mid-text", () => {
    expect(detectSlashQuery("look at src/lib")).toBeNull();
    expect(detectSlashQuery(" /plan")).toBeNull();
  });

  it("closes the picker once an argument is being typed", () => {
    // The command is still valid — see parseSlashCommand — but the user is no
    // longer choosing from the list.
    expect(detectSlashQuery("/review HEAD~3")).toBeNull();
    expect(detectSlashQuery("hello")).toBeNull();
  });
});

describe("parseSlashCommand", () => {
  it("parses a bare command", () => {
    const parsed = parseSlashCommand("/compact");
    expect(parsed?.command.id).toBe("compact");
    expect(parsed?.argument).toBe("");
  });

  it("parses a command with an argument", () => {
    const parsed = parseSlashCommand("/rename Tauri bridge notes");
    expect(parsed?.command.id).toBe("rename");
    expect(parsed?.argument).toBe("Tauri bridge notes");
  });

  it("is case-insensitive and tolerates surrounding whitespace", () => {
    expect(parseSlashCommand("  /REVIEW  the auth changes  ")).toEqual({
      command: SLASH_COMMANDS.find((command) => command.id === "review"),
      argument: "the auth changes",
    });
  });

  it("returns null for ordinary text and unknown commands", () => {
    expect(parseSlashCommand("just a message")).toBeNull();
    expect(parseSlashCommand("/nope")).toBeNull();
    // A bare slash names no command, so it is ordinary text.
    expect(parseSlashCommand("/")).toBeNull();
  });
});

describe("filterSlashCommands", () => {
  it("returns all commands for an empty query", () => {
    expect(filterSlashCommands("")).toEqual(SLASH_COMMANDS);
  });

  it("filters by prefix, case-insensitively", () => {
    expect(filterSlashCommands("Pl").map((command) => command.id)).toEqual(["plan"]);
  });

  it("falls back to matching the description", () => {
    expect(filterSlashCommands("sum").map((command) => command.id)).toEqual(["compact"]);
  });

  it("ranks prefix matches above description matches", () => {
    // "model" matches its own id by prefix; "permissions" only by description.
    const ids = filterSlashCommands("model").map((command) => command.id);
    expect(ids[0]).toBe("model");
  });

  it("returns nothing for an unknown command", () => {
    expect(filterSlashCommands("zzz")).toEqual([]);
  });
});
