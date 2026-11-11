/**
 * Worktree context: what a new thread's first message tells the agent about
 * the managed PR worktree or Stack worktree it starts in.
 *
 * It travels as text inside the user's message, wrapped in a tag, because that
 * is the one channel every harness has. Harnesses hand the message back as
 * one string on reload, so the block is found by its tag, not by which part it
 * was sent in. This file is the only frontend owner of the tag; the backend
 * builds the text and strips the block for titles
 * (`src-tauri/src/review/worktree_context.rs`).
 */

import type { UserInputPart } from "$lib/types";

/** Mirror `OPEN` and `CLOSE` in `src-tauri/src/review/worktree_context.rs`. */
const OPEN = "<worktree_context>";
const CLOSE = "</worktree_context>";
const BLOCK = new RegExp(`\\s*${OPEN}\\n?([\\s\\S]*?)\\n?${CLOSE}`);

/** `input` with the context appended as its last text part: after the user's
 *  words, so the message's first line still titles the thread. */
export function withWorktreeContext(input: UserInputPart[], text: string): UserInputPart[] {
  return [...input, { type: "text", text: `\n\n${OPEN}\n${text}\n${CLOSE}` }];
}

/** A message's own content and the worktree context sent with it, if any. */
export function splitWorktreeContext(content: UserInputPart[]): {
  content: UserInputPart[];
  context: string | null;
} {
  let context: string | null = null;
  const rest: UserInputPart[] = [];
  for (const part of content) {
    const match: RegExpExecArray | null = context === null && part.type === "text" ? BLOCK.exec(part.text ?? "") : null;
    if (!match) {
      rest.push(part);
      continue;
    }
    context = match[1];
    const text = (part.text ?? "").replace(BLOCK, "");
    if (text) rest.push({ ...part, text });
  }
  return { content: rest, context };
}
