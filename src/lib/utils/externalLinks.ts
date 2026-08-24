import { openExternalUrl, revealInFinder } from "$lib/services/api";

/** Anchors carrying one of these schemes are handed to the OS browser. */
const EXTERNAL_SCHEMES = ["http:", "https:", "mailto:"];

let installed = false;

/**
 * Install a document-level click interceptor so links rendered anywhere in the
 * app (chiefly markdown output) never navigate the Tauri webview — which would
 * replace the app with no way back. Web/mail links open in the default
 * browser, local file paths are revealed in Finder, and anything else is
 * swallowed.
 */
export function installExternalLinkHandler(): void {
  if (installed || typeof document === "undefined") return;
  installed = true;

  const handle = (event: MouseEvent) => {
    if (event.defaultPrevented) return;
    // Primary (left) and middle clicks both mean "open this link"; ignore
    // right-click (context menu) and other buttons.
    if (event.button !== 0 && event.button !== 1) return;

    const anchor = (event.target as HTMLElement | null)?.closest?.("a");
    if (!(anchor instanceof HTMLAnchorElement)) return;

    const href = anchor.getAttribute("href");
    if (!href) return;
    // Same-document fragments are in-app navigation; leave them alone.
    if (href.startsWith("#")) return;

    event.preventDefault();

    // Bare absolute / home-relative paths (e.g. a model emitting
    // `[out.csv](/Users/me/out.csv)`) would otherwise resolve to
    // `tauri://localhost/Users/...` and blank the window.
    if (href.startsWith("/") || href.startsWith("~/")) {
      revealInFinder(safeDecode(href)).catch(() => {});
      return;
    }

    // Only hrefs that carry their own scheme can be external; a relative
    // path resolved against the app origin must never leave the webview.
    if (!/^[a-z][a-z0-9+.-]*:/i.test(href)) return;
    let url: URL;
    try {
      url = new URL(href);
    } catch {
      return;
    }
    if (EXTERNAL_SCHEMES.includes(url.protocol)) {
      openExternalUrl(url.href).catch(() => {});
    } else if (url.protocol === "file:") {
      revealInFinder(safeDecode(url.pathname)).catch(() => {});
    }
    // Anything else (relative paths, unknown schemes) is dropped on the floor.
  };

  // Capture phase so we win before any in-app navigation kicks in. Middle
  // clicks arrive as `auxclick`, not `click`, so listen for both.
  document.addEventListener("click", handle, true);
  document.addEventListener("auxclick", handle, true);
}

function safeDecode(path: string): string {
  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}
