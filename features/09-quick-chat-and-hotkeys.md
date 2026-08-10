# Quick chat and global hotkeys

Priority: P2

## What it should do

Open a lightweight composer from a global shortcut, send a quick question without bringing the full app forward, and hand the resulting thread back to the main window.

## How

Use Tauri's global-shortcut support and a separate small window route. Share session, model, permission, and draft services with the main app. Persist the chosen shortcut, handle conflicts, and never allow the quick window to create a second Codex session accidentally.

## What it should look like

Make the quick window a compact floating composer with project selector, model selector, attachment affordance, and Send. It should feel instant, dismiss on Escape, show a short streaming response, and offer `Open full thread` when work continues.
