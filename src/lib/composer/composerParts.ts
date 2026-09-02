/**
 * The composer's parts model and the pure functions over it: what the editor
 * holds, independent of any DOM. `richInput.ts` renders these to and from a
 * contenteditable; `richEditor.svelte.ts` owns the live instance.
 */
import type { TurnInputItem } from "$lib/types";
import { relativeMentionPath } from "$lib/utils/mentions";

/** Progress of a staged attachment: mid-copy, ready to send, or copy failed. */
export type AttachmentState = "staging" | "ready" | "failed";

/** A file or image the user attached, rendered as an inline chip at the caret. */
export interface AttachmentPart {
  type: "attachment";
  id: string;
  filename: string;
  mime: string;
  size: number;
  /** Staged copy path (empty while staging/failed). */
  path: string;
  kind: "image" | "file";
  state: AttachmentState;
}

export type ComposerPart =
  | { type: "text"; text: string }
  | { type: "mention"; name: string; path: string }
  | { type: "skill"; name: string; path: string; label: string }
  | AttachmentPart;

/** Callbacks wired into every attachment chip's controls. */
export interface AttachmentChipHandlers {
  onRetry: (id: string) => void;
  /** Resolves an image chip's thumbnail src (asset URL / blob), or null. */
  thumbSrc?: (part: AttachmentPart) => string | null;
}

/** Trigger-character queries detected before the caret. A `Range` covers the
 *  trigger plus its query so a pick can replace them with a chip. */
export interface DetectedQueries {
  slashQuery: string | null;
  mentionQuery: string | null;
  mentionRange: Range | null;
  skillQuery: string | null;
  skillRange: Range | null;
}

export const EMPTY_PARTS = (): ComposerPart[] => [{ type: "text", text: "" }];

export const NO_QUERIES = (): DetectedQueries => ({
  slashQuery: null,
  mentionQuery: null,
  mentionRange: null,
  skillQuery: null,
  skillRange: null,
});

/** Human-readable file size for chip labels (e.g. "12 KB", "3.4 MB"). */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function normaliseParts(input: ComposerPart[]): ComposerPart[] {
  const normalised: ComposerPart[] = [];
  for (const part of input) {
    if (part.type === "text") {
      if (!part.text) continue;
      const previous = normalised.at(-1);
      if (previous?.type === "text") previous.text += part.text;
      else normalised.push(part);
    } else {
      normalised.push(part);
    }
  }
  return normalised.length > 0 ? normalised : EMPTY_PARTS();
}

/**
 * Turns composer parts into the `turn/start` input array. Images that finished
 * staging become native `localImage` items; non-image files are appended as a
 * clearly-labelled text reference (the protocol has no native file item), in
 * their original position. Attachments still staging or failed are dropped.
 *
 * File mentions go out as cwd-relative markdown links, the form Codex itself
 * writes and round-trips. The protocol's `mention` item looks like a fit but is
 * not one: its `path` is a connector/plugin target (`app://…`, `plugin://…`), so
 * a filesystem path resolves to nothing and is dropped without an error.
 *
 * Skills, by contrast, do have a native item — `{type:"skill", name, path}` —
 * so they go out as themselves rather than as text. (`path` is required by the
 * server; `TurnInputItem` makes leaving it out a type error.)
 */
export function buildTurnInput(parts: ComposerPart[], cwd = ""): TurnInputItem[] {
  const input: TurnInputItem[] = [];
  for (const part of parts) {
    if (part.type === "text") {
      input.push({ type: "text", text: part.text });
    } else if (part.type === "mention") {
      input.push({ type: "text", text: `[${part.name}](${relativeMentionPath(part.path, cwd)})` });
    } else if (part.type === "skill") {
      input.push({ type: "skill", name: part.name, path: part.path });
    } else if (part.type === "attachment" && part.state === "ready") {
      if (part.kind === "image") {
        input.push({ type: "localImage", path: part.path });
      } else {
        input.push({ type: "text", text: `\n[Attached file: ${part.filename} — ${part.path}]\n` });
      }
    }
  }
  return input.length > 0 ? input : [{ type: "text", text: "" }];
}

/** Whether any part carries sendable content (text or a ready attachment). */
export function hasSendableContent(parts: ComposerPart[]): boolean {
  return parts.some(
    (part) =>
      (part.type === "text" && part.text.trim().length > 0) ||
      part.type === "mention" ||
      part.type === "skill" ||
      (part.type === "attachment" && part.state === "ready"),
  );
}

/**
 * Flattens composer parts into a goal objective. A goal is a plain string on
 * the Codex side, so chips have to become text: mentions as the same
 * cwd-relative links a turn sends, skills as their name plus the skill file so
 * the goal loop can open it, non-image files as a labelled path reference.
 * Images have no textual form and are dropped.
 */
export function buildGoalObjective(parts: ComposerPart[], cwd = ""): string {
  let objective = "";
  // Chips sit flush against their neighbours in the editor (picking one eats
  // the trigger and its space), so the flattened form separates them itself.
  const chip = (text: string) => {
    if (objective && !/\s$/.test(objective)) objective += " ";
    objective += text;
  };
  for (const part of parts) {
    if (part.type === "text") {
      if (objective && !/\s$/.test(objective) && part.text && !/^\s/.test(part.text)) objective += " ";
      objective += part.text;
    } else if (part.type === "mention") {
      chip(`[${part.name}](${relativeMentionPath(part.path, cwd)})`);
    } else if (part.type === "skill") {
      chip(`$${part.name} (skill: ${relativeMentionPath(part.path, cwd)})`);
    } else if (part.type === "attachment" && part.state === "ready" && part.kind === "file") {
      objective += `\n[Attached file: ${part.filename} — ${part.path}]\n`;
    }
  }
  return objective.trim();
}
