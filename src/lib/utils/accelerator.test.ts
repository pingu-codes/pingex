import { describe, expect, it } from "vitest";
import { acceleratorFromEvent } from "$lib/utils/accelerator";

function keydown(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("acceleratorFromEvent", () => {
  it("maps meta to CmdOrCtrl and uppercases letters", () => {
    expect(acceleratorFromEvent(keydown({ key: "k", metaKey: true }))).toBe("CmdOrCtrl+K");
  });

  it("orders modifiers and names the space key", () => {
    expect(acceleratorFromEvent(keydown({ key: " ", metaKey: true, shiftKey: true }))).toBe("CmdOrCtrl+Shift+Space");
  });

  it("uses Control only when meta is absent", () => {
    expect(acceleratorFromEvent(keydown({ key: "j", ctrlKey: true, altKey: true }))).toBe("Control+Alt+J");
  });

  it("passes through function and arrow keys", () => {
    expect(acceleratorFromEvent(keydown({ key: "F5", altKey: true }))).toBe("Alt+F5");
    expect(acceleratorFromEvent(keydown({ key: "ArrowUp", metaKey: true }))).toBe("CmdOrCtrl+Up");
  });

  it("returns null while only modifiers are held", () => {
    expect(acceleratorFromEvent(keydown({ key: "Meta", metaKey: true }))).toBeNull();
    expect(acceleratorFromEvent(keydown({ key: "Shift", shiftKey: true }))).toBeNull();
  });
});
