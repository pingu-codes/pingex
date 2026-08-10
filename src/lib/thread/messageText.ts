/**
 * Helpers for reading a sent user message back out of its `UserInputPart[]`.
 *
 * Mentions leave the composer as their own text part (`buildTurnInput` emits
 * `[a.ts](src/a.ts)` separately from the prose either side of it), so a message
 * arrives here split across several adjacent text parts. Rendering each one on
 * its own would break a sentence into lines mid-mention, so they are merged
 * back into single runs first.
 */

import type { UserInputPart } from "$lib/types";
import { copyMentionPath, splitMentions } from "$lib/utils/mentions";

const basename = (path: string) => path.replace(/\/+$/, "").split("/").pop() || path;

/** Merges adjacent text parts into one run, leaving other parts untouched. */
export function mergeTextParts(content: UserInputPart[]): UserInputPart[] {
  const merged: UserInputPart[] = [];
  for (const part of content) {
    const previous = merged.at(-1);
    if (part.type === "text" && previous?.type === "text") {
      merged[merged.length - 1] = { ...previous, text: (previous.text ?? "") + (part.text ?? "") };
    } else {
      merged.push(part);
    }
  }
  return merged;
}

/** The message's prose, as one string — what the edit textarea is seeded with. */
export function messageText(content: UserInputPart[]): string {
  return content
    .filter((part) => part.type === "text")
    .map((part) => part.text ?? "")
    .join("");
}

/**
 * The message as markdown for the clipboard: file mentions keep their link form
 * (rewritten to `./`-relative paths), and attachments become links of the same
 * shape so a pasted message still points at every file it referenced.
 */
export function userMessageMarkdown(content: UserInputPart[], cwd = ""): string {
  let out = "";
  const block = (text: string) => {
    if (out && !out.endsWith("\n")) out += "\n";
    out += `${text}\n`;
  };
  for (const part of mergeTextParts(content)) {
    if (part.type === "text") {
      out += splitMentions(part.text ?? "")
        .map((segment) =>
          segment.type === "mention" ? `[${segment.name}](${copyMentionPath(segment.path, cwd)})` : segment.text,
        )
        .join("");
    } else if (part.type === "image" && part.url) {
      block(`[image](${part.url})`);
    } else if (part.path) {
      block(`[${part.name ?? basename(part.path)}](${copyMentionPath(part.path, cwd)})`);
    } else if (part.name) {
      block(`@${part.name}`);
    }
  }
  return out.trim();
}
