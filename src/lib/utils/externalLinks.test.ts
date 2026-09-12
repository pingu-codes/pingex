import { beforeEach, describe, expect, it, vi } from "vitest";

const openExternalUrl = vi.fn((_url: string) => Promise.resolve());
const revealInFinder = vi.fn((_path: string) => Promise.resolve());

vi.mock("$lib/services/api", () => ({
  openExternalUrl: (url: string) => openExternalUrl(url),
  revealInFinder: (path: string) => revealInFinder(path),
}));

const { installExternalLinkHandler } = await import("./externalLinks");

const onFilePath = vi.fn();

function click(href: string): boolean {
  const anchor = document.createElement("a");
  anchor.setAttribute("href", href);
  document.body.appendChild(anchor);
  const event = new MouseEvent("click", { bubbles: true, cancelable: true, button: 0 });
  anchor.dispatchEvent(event);
  anchor.remove();
  return event.defaultPrevented;
}

const ref = (path: string, line: number | null = null, column: number | null = null) => ({ path, line, column });

/**
 * Must come first: the built-in fallback is only observable until the app
 * injects a handler, and a handler once installed stays installed.
 */
describe("installExternalLinkHandler, before a handler is injected", () => {
  it("reveals file paths in Finder, stripping the cited line", () => {
    installExternalLinkHandler();
    click("/a/b.ts:100");
    expect(revealInFinder).toHaveBeenCalledWith("/a/b.ts");
  });
});

describe("installExternalLinkHandler", () => {
  beforeEach(() => {
    installExternalLinkHandler({ onFilePath });
    openExternalUrl.mockClear();
    revealInFinder.mockClear();
    onFilePath.mockClear();
  });

  it("opens web links in the browser", () => {
    expect(click("https://example.com/x")).toBe(true);
    expect(openExternalUrl).toHaveBeenCalledWith("https://example.com/x");
    expect(onFilePath).not.toHaveBeenCalled();
  });

  it("hands absolute file paths to the file handler instead of navigating", () => {
    expect(click("/Users/x/y.csv")).toBe(true);
    expect(onFilePath).toHaveBeenCalledWith(ref("/Users/x/y.csv"));
    expect(openExternalUrl).not.toHaveBeenCalled();
  });

  it("passes the cited line and column through", () => {
    click("/a/b.ts:100");
    expect(onFilePath).toHaveBeenCalledWith(ref("/a/b.ts", 100));
    onFilePath.mockClear();
    click("/a/b.ts:100:5");
    expect(onFilePath).toHaveBeenCalledWith(ref("/a/b.ts", 100, 5));
  });

  it("hands file: URLs to the file handler, decoded", () => {
    expect(click("file:///Users/x/my%20file.csv")).toBe(true);
    expect(onFilePath).toHaveBeenCalledWith(ref("/Users/x/my file.csv"));
  });

  it("keeps home-relative paths intact", () => {
    expect(click("~/.codex/foo")).toBe(true);
    expect(onFilePath).toHaveBeenCalledWith(ref("~/.codex/foo"));
  });

  it("leaves same-document fragments alone", () => {
    expect(click("#section")).toBe(false);
    expect(openExternalUrl).not.toHaveBeenCalled();
    expect(onFilePath).not.toHaveBeenCalled();
  });

  it("swallows relative links and unknown schemes", () => {
    expect(click("relative/path")).toBe(true);
    expect(click("javascript:alert(1)")).toBe(true);
    expect(openExternalUrl).not.toHaveBeenCalled();
    expect(onFilePath).not.toHaveBeenCalled();
  });

  /**
   * The listeners attach once, but the handler must stay reconfigurable — if
   * the `installed` guard short-circuited the options, the app would keep
   * revealing in Finder while these tests passed.
   */
  it("keeps the handler reconfigurable after the listeners are attached", () => {
    installExternalLinkHandler();
    click("/a/b.ts:100");
    // A call with no handler leaves the installed one in force.
    expect(onFilePath).toHaveBeenCalledWith(ref("/a/b.ts", 100));
    expect(revealInFinder).not.toHaveBeenCalled();

    const second = vi.fn();
    installExternalLinkHandler({ onFilePath: second });
    click("/c/d.ts");
    expect(second).toHaveBeenCalledWith(ref("/c/d.ts"));
  });
});
