# Settings and configuration

Priority: P0

## What it should do

Provide one place to manage Codex behavior, appearance, integrations, account state, and local runtime settings. Support global, project, and managed values, with clear read-only indicators and restart notices.

## How

Replace the single modal in `src/lib/layout/SettingsDialog.svelte` with a routed settings shell. Add typed Rust commands for reading/writing supported `config.toml` values, using `src-tauri/src/settings.rs` for local overrides and app-server requests for runtime-owned settings. Keep secrets in native storage and preserve explicit unset versus inherited values.

## What it should look like

Use a two-pane desktop layout: searchable settings navigation on the left, grouped cards on the right. Include sections for General, Appearance, Agent, Model features, Integrations, Coding, Connections, Keyboard shortcuts, and Data controls. Every control shows its source, scope, current value, and whether restart is required.
