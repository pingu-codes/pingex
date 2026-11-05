import { describe, expect, it } from "vitest";
import { acceleratorFromEvent, formatAccelerator } from "$lib/utils/accelerator";

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

describe("formatAccelerator", () => {
  it("writes an accelerator the way a menu hint shows it", () => {
    expect(formatAccelerator("Enter")).toBe("↵");
    expect(formatAccelerator("T")).toBe("T");
    expect(formatAccelerator("CmdOrCtrl+Shift+C")).toBe("⌘⇧C");
    expect(formatAccelerator("Alt+Up")).toBe("⌥↑");
    expect(formatAccelerator("Shift+V")).toBe("⇧V");
    expect(formatAccelerator("F2")).toBe("F2");
  });

  it("round-trips with the keydown that produces it", () => {
    const shortcut = acceleratorFromEvent(keydown({ key: "ArrowDown", altKey: true }));
    expect(shortcut).toBe("Alt+Down");
    expect(formatAccelerator(shortcut ?? "")).toBe("⌥↓");
  });
});
