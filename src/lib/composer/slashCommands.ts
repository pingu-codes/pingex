/** Slash commands available from the composer, mirroring the Codex TUI set. */

export type SlashCommandId =
  | "plan"
  | "model"
  | "permissions"
  | "compact"
  | "new"
  | "fork"
  | "archive"
  | "rename"
  | "review"
  | "diff"
  | "init"
  | "status"
  | "mcp"
  | "skills"
  | "undo"
  | "goal"
  | "copy"
  | "export"
  | "delete";

export interface SlashCommand {
  id: SlashCommandId;
  description: string;
  /**
   * Where the command is handled:
   * - `composer` — entirely inside the Composer (toggles, popovers, prefills).
   * - `thread` — needs the live thread, so it bubbles up via `onCommand`.
   * - `settings` — deep-links into a Settings section; no thread required.
   */
  scope: "composer" | "thread" | "settings";
  /** What a trailing argument means, shown as a hint once one is being typed. */
  argument?: string;
}

export const SLASH_COMMANDS: SlashCommand[] = [
  { id: "plan", description: "Toggle plan mode for the next turns", scope: "composer" },
  { id: "model", description: "Choose the model and reasoning effort", scope: "composer" },
  { id: "permissions", description: "Set the approval and sandbox level", scope: "composer" },
  { id: "init", description: "Write an AGENTS.md for this project", scope: "composer" },
  { id: "compact", description: "Summarise the thread to free up context", scope: "thread" },
  { id: "new", description: "Start a new thread", scope: "thread" },
  { id: "fork", description: "Fork the current thread", scope: "thread" },
  { id: "archive", description: "Archive the current thread", scope: "thread" },
  { id: "rename", description: "Rename the current thread", scope: "thread", argument: "new name" },
  { id: "review", description: "Review the working changes", scope: "thread", argument: "what to review" },
  { id: "diff", description: "Show the working diff against the remote", scope: "thread" },
  { id: "undo", description: "Rewind the last turn", scope: "thread", argument: "turns to drop" },
  {
    id: "goal",
    description: "Set or view the goal for a long-running task",
    scope: "thread",
    argument: "objective, or clear",
  },
  { id: "copy", description: "Copy the last response as markdown", scope: "thread" },
  { id: "export", description: "Copy the conversation as markdown", scope: "thread" },
  { id: "delete", description: "Permanently delete the current thread", scope: "thread" },
  { id: "status", description: "Show usage, limits, and context for this thread", scope: "thread" },
  { id: "mcp", description: "Manage MCP servers and their tools", scope: "settings" },
  { id: "skills", description: "Browse and enable skills", scope: "settings" },
];

/**
 * The command query while one is still being typed — a leading "/" followed by
 * word characters only — or null.
 *
 * Returns null as soon as a space is typed, which closes the picker: at that
 * point the user is writing an argument, not choosing a command. Use
 * `parseSlashCommand` to read the finished command and its argument.
 */
export function detectSlashQuery(text: string): string | null {
  const match = text.match(/^\/([\w-]*)$/);
  return match ? match[1] : null;
}

export interface ParsedSlashCommand {
  command: SlashCommand;
  /** Text after the command name, trimmed. Empty when none was given. */
  argument: string;
}

/**
 * Resolve submitted composer text to a command, with any argument. Returns null
 * when the text is not a slash command or names one that does not exist, so the
 * caller can send it as an ordinary message.
 */
export function parseSlashCommand(text: string): ParsedSlashCommand | null {
  const match = text.trim().match(/^\/([\w-]+)(?:\s+([\s\S]*))?$/);
  if (!match) return null;
  const id = match[1].toLowerCase();
  const command = SLASH_COMMANDS.find((candidate) => candidate.id === id);
  return command ? { command, argument: (match[2] ?? "").trim() } : null;
}

/**
 * Commands matching the query, best matches first. Matches the id by prefix,
 * then anywhere in the id or description — so `/sum` finds `compact` by its
 * "Summarise the thread" wording rather than coming up empty.
 */
export function filterSlashCommands(query: string): SlashCommand[] {
  const lowered = query.trim().toLowerCase();
  if (!lowered) return SLASH_COMMANDS;
  const prefix: SlashCommand[] = [];
  const elsewhere: SlashCommand[] = [];
  for (const command of SLASH_COMMANDS) {
    if (command.id.startsWith(lowered)) prefix.push(command);
    else if (command.id.includes(lowered) || command.description.toLowerCase().includes(lowered)) {
      elsewhere.push(command);
    }
  }
  return [...prefix, ...elsewhere];
}

/** The prompt `/init` prefills. Mirrors what the Codex TUI asks for. */
export const INIT_PROMPT =
  "Analyse this codebase and write an AGENTS.md at the repo root. Cover the build, " +
  "test, and lint commands; the architecture and where the important code lives; and " +
  "any conventions a new contributor would otherwise get wrong. If an AGENTS.md " +
  "already exists, improve it in place rather than replacing it.";
