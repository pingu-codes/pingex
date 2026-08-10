import { openExternalUrl } from "$lib/services/api";

/** Anchors carrying one of these schemes are handed to the OS browser. */
const EXTERNAL_SCHEMES = ["http:", "https:", "mailto:"];

let installed = false;

/**
 * Install a document-level click interceptor so links rendered anywhere in the
 * app (chiefly markdown output) open in the user's default browser instead of
 * navigating the Tauri webview — which would replace the app with no way back.
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

    let url: URL;
    try {
      url = new URL(href, window.location.href);
    } catch {
      return;
    }
    if (!EXTERNAL_SCHEMES.includes(url.protocol)) return;

    event.preventDefault();
    openExternalUrl(url.href).catch(() => {});
  };

  // Capture phase so we win before any in-app navigation kicks in. Middle
  // clicks arrive as `auxclick`, not `click`, so listen for both.
  document.addEventListener("click", handle, true);
  document.addEventListener("auxclick", handle, true);
}
