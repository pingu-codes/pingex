import { describe, expect, it } from "vitest";
import { describeBrowserCall } from "$lib/browser/browserActions";

describe("describeBrowserCall", () => {
  it("says what the agent did in a few words", () => {
    expect(describeBrowserCall("open", { url: "https://www.github.com/openai/codex" })).toBe(
      "Open github.com/openai/codex",
    );
    expect(describeBrowserCall("navigate", { tab: "t1", url: "back" })).toBe("Back in t1");
    expect(describeBrowserCall("click", { tab: "t2", ref: "e12" })).toBe("Click e12 in t2");
    expect(describeBrowserCall("type", { tab: "t1", ref: "e5", text: "hello" })).toBe("Type “hello” into e5 in t1");
    expect(describeBrowserCall("screenshot", { tab: "t1", fullPage: true })).toBe("Screenshot of the full page in t1");
    expect(describeBrowserCall("take", {})).toBe("Take over the active tab");
    expect(describeBrowserCall("cdp", { tab: "browser", method: "Browser.getVersion" })).toBe(
      "CDP Browser.getVersion on the browser",
    );
    expect(describeBrowserCall("open", { url: "file:///tmp/site/index.html" })).toBe("Open index.html");
  });

  it("clips long typed text", () => {
    expect(describeBrowserCall("type", { ref: "e1", text: "x".repeat(60) })).toBe(`Type “${"x".repeat(39)}…” into e1`);
  });
});
