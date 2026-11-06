# Pingex Browser Helper

This Chrome extension keeps the tabs a Pingex thread owns in a tab group,
named after the thread. Pingex drives the browser over the DevTools Protocol,
but CDP cannot create or read tab groups, so this extension handles that part.

## Install

Pingex builds these files into the app and installs them itself. Click
**Install helper** in Settings → Browser, or in a thread's Browser panel. That
copies them to Pingex's app-data folder
(`~/Library/Application Support/pingex/browser-helper` on macOS) and opens it.
Then:

1. Open `chrome://extensions` and turn on **Developer mode**.
2. Click **Load unpacked** and choose that folder.

This folder (`extensions/pingex-chrome`) is the source. Don't load it from a
checkout, which may move or be deleted.

The extension id is fixed by the manifest `key`:
`moebbciiianafkbmljnjplaggnhgpken`. Pingex only trusts that id, so if you
change the key, also change `HELPER_EXTENSION_ID` in
`src-tauri/src/browser/helper.rs`.

## How Pingex talks to it

- There is no native-messaging host, and the extension makes no network
  requests.
- Pingex attaches to its service worker as a CDP target, over the connection
  it already has, and calls the functions on `globalThis.pingex` with
  `Runtime.evaluate`.
- The extension reports group changes you make by hand back through the
  `pingexNotify` binding that Pingex installs.

The `debugger` permission is used only for `chrome.debugger.getTargets()`.
That call maps a CDP target id to a Chrome tab id; nothing else can provide
that mapping. The extension never attaches a debugger itself.
