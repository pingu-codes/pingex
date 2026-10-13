import { openExternalUrl, revealInFinder } from "$lib/services/api";
import { type FileRef, parseFileRef } from "$lib/utils/fileRef";

/** Anchors carrying one of these schemes are handed to the OS browser. */
const EXTERNAL_SCHEMES = ["http:", "https:", "mailto:"];

/** What to do with a link that turns out to name a local file. */
type LinkHandlers = { onFilePath?: (ref: FileRef) => void };

let installed = false;
let handlers: Required<LinkHandlers> = { onFilePath: (ref) => void revealInFinder(ref.path).catch(() => {}) };

/**
 * Install a document-level click interceptor so links rendered anywhere in the
 * app (chiefly markdown output) never navigate the Tauri webview — which would
 * replace the app with no way back. Web/mail links open in the default
 * browser, local file paths go to `onFilePath`, and anything else is swallowed.
 *
 * This module stays free of Svelte and dialog imports so it can be tested as
 * plain DOM; the app injects the preview modal through `onFilePath`.
 */
export function installExternalLinkHandler(options: LinkHandlers = {}): void {
  if (typeof document === "undefined") return;
  // Reconfiguring is allowed even after the listeners are attached: only the
  // `addEventListener` calls are once-only. Folding this into the guard below
  // would leave a second call silently doing nothing.
  if (options.onFilePath) handlers = { onFilePath: options.onFilePath };
  if (installed) return;
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
      handlers.onFilePath(parseFileRef(href));
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
      handlers.onFilePath(parseFileRef(url.pathname));
    }
    // Anything else (relative paths, unknown schemes) is dropped on the floor.
  };

  // Capture phase so we win before any in-app navigation kicks in. Middle
  // clicks arrive as `auxclick`, not `click`, so listen for both.
  document.addEventListener("click", handle, true);
  document.addEventListener("auxclick", handle, true);
}
