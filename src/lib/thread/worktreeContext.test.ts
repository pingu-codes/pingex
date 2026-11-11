import { describe, expect, it } from "vitest";
import { splitWorktreeContext, withWorktreeContext } from "$lib/thread/worktreeContext";
import type { UserInputPart } from "$lib/types";

const CONTEXT = 'This directory is the worktree of the stack "billing".\n1. #101 Add the ledger (`billing/ledger`)';
const typed: UserInputPart[] = [
  { type: "text", text: "Fix the tax bug in " },
  { type: "text", text: "[tax.ts](src/tax.ts)" },
];
const sent = withWorktreeContext(typed, CONTEXT);
const texts = (parts: UserInputPart[]) => parts.map((part) => part.text ?? "");

describe("worktree context", () => {
  it("goes after the user's words, as a part of its own", () => {
    expect(sent.slice(0, 2)).toEqual(typed);
    expect(sent).toHaveLength(3);
    expect(splitWorktreeContext(sent)).toEqual({ content: typed, context: CONTEXT });
  });

  it("is found when a harness hands the message back as one string", () => {
    // Codex joins text parts with nothing, Claude and ACP with a newline.
    for (const separator of ["", "\n"]) {
      const reloaded: UserInputPart[] = [{ type: "text", text: texts(sent).join(separator) }];
      const split = splitWorktreeContext(reloaded);
      expect(split.context).toBe(CONTEXT);
      expect(texts(split.content).join("").trim()).toBe(texts(typed).join(separator).trim());
    }
  });

  it("keeps attachments that follow it", () => {
    const image: UserInputPart = { type: "image", url: "data:image/png;base64,AA" };
    const split = splitWorktreeContext([...sent, image]);
    expect(split.context).toBe(CONTEXT);
    expect(split.content).toEqual([...typed, image]);
  });

  it("leaves a message without context alone", () => {
    expect(splitWorktreeContext(typed)).toEqual({ content: typed, context: null });
    const unclosed: UserInputPart[] = [{ type: "text", text: "What does <worktree_context> mean?" }];
    expect(splitWorktreeContext(unclosed)).toEqual({ content: unclosed, context: null });
  });
});
