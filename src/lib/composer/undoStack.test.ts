import { describe, expect, it } from "vitest";
import type { ComposerPart } from "./richInput";
import { COALESCE_MS, UndoStack } from "./undoStack";

const text = (t: string): ComposerPart[] => [{ type: "text", text: t }];

describe("UndoStack", () => {
  it("undoes and redoes through recorded states", () => {
    const stack = new UndoStack(() => 0);
    stack.reset({ parts: text(""), caret: 0 });
    stack.record({ parts: text("a"), caret: 1 });
    stack.record({ parts: text("ab"), caret: 2 });
    expect(stack.undo()?.parts).toEqual(text("a"));
    expect(stack.undo()?.parts).toEqual(text(""));
    expect(stack.undo()).toBeNull();
    expect(stack.redo()?.parts).toEqual(text("a"));
    expect(stack.redo()?.parts).toEqual(text("ab"));
    expect(stack.redo()).toBeNull();
  });

  it("coalesces quick typing into one entry but keeps the first keystroke apart", () => {
    let t = 0;
    const stack = new UndoStack(() => t);
    stack.reset({ parts: text(""), caret: 0 });
    stack.record({ parts: text("h"), caret: 1 }, true);
    t += 100;
    stack.record({ parts: text("he"), caret: 2 }, true);
    t += 100;
    stack.record({ parts: text("hey"), caret: 3 }, true);
    t += COALESCE_MS + 1;
    stack.record({ parts: text("hey!"), caret: 4 }, true);
    expect(stack.undo()?.parts).toEqual(text("hey"));
    expect(stack.undo()?.parts).toEqual(text(""));
  });

  it("clears redo on a new edit and ignores unchanged content", () => {
    const stack = new UndoStack(() => 0);
    stack.reset({ parts: text(""), caret: 0 });
    stack.record({ parts: text("a"), caret: 1 });
    stack.undo();
    stack.record({ parts: text("b"), caret: 1 });
    expect(stack.canRedo).toBe(false);
    stack.record({ parts: text("b"), caret: 0 });
    expect(stack.current?.caret).toBe(0);
    expect(stack.undo()?.parts).toEqual(text(""));
  });
});
