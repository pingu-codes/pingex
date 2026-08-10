# History search and pagination

Priority: P2

## What it should do

Search all available threads and archived history without imposing a small fixed result cap. Load results incrementally and keep the sidebar responsive on large Codex homes.

## How

Preserve app-server cursors in the Rust session layer and expose page-based APIs to Svelte. Add a local Turso search index for titles, previews, project paths, and timestamps; update it from thread events and invalidate stale rows. Search should be cancellable and generation-safe.

## What it should look like

Add a sidebar search command with keyboard focus and grouped results. Show loading, no-match, and unavailable states. Use `Load more` or infinite scrolling for active and archived results, with result counts and a clear indication of the current project or status filter.
