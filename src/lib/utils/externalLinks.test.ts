import { beforeEach, describe, expect, it, vi } from "vitest";

const openExternalUrl = vi.fn((_url: string) => Promise.resolve());
const revealInFinder = vi.fn((_path: string) => Promise.resolve());

vi.mock("$lib/services/api", () => ({
  openExternalUrl: (url: string) => openExternalUrl(url),
  revealInFinder: (path: string) => revealInFinder(path),
}));

const { installExternalLinkHandler } = await import("./externalLinks");

function click(href: string): boolean {
  const anchor = document.createElement("a");
  anchor.setAttribute("href", href);
  document.body.appendChild(anchor);
  const event = new MouseEvent("click", { bubbles: true, cancelable: true, button: 0 });
  anchor.dispatchEvent(event);
  anchor.remove();
  return event.defaultPrevented;
}

describe("installExternalLinkHandler", () => {
  beforeEach(() => {
    installExternalLinkHandler();
    openExternalUrl.mockClear();
    revealInFinder.mockClear();
  });

  it("opens web links in the browser", () => {
    expect(click("https://example.com/x")).toBe(true);
    expect(openExternalUrl).toHaveBeenCalledWith("https://example.com/x");
    expect(revealInFinder).not.toHaveBeenCalled();
  });

  it("reveals absolute file paths in Finder instead of navigating", () => {
    expect(click("/Users/x/y.csv")).toBe(true);
    expect(revealInFinder).toHaveBeenCalledWith("/Users/x/y.csv");
    expect(openExternalUrl).not.toHaveBeenCalled();
  });

  it("reveals file: URLs in Finder", () => {
    expect(click("file:///Users/x/my%20file.csv")).toBe(true);
    expect(revealInFinder).toHaveBeenCalledWith("/Users/x/my file.csv");
  });

  it("keeps home-relative paths intact", () => {
    expect(click("~/.codex/foo")).toBe(true);
    expect(revealInFinder).toHaveBeenCalledWith("~/.codex/foo");
  });

  it("leaves same-document fragments alone", () => {
    expect(click("#section")).toBe(false);
    expect(openExternalUrl).not.toHaveBeenCalled();
    expect(revealInFinder).not.toHaveBeenCalled();
  });

  it("swallows relative links and unknown schemes", () => {
    expect(click("relative/path")).toBe(true);
    expect(click("javascript:alert(1)")).toBe(true);
    expect(openExternalUrl).not.toHaveBeenCalled();
    expect(revealInFinder).not.toHaveBeenCalled();
  });
});
