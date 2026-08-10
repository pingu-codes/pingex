# Project sources and search

Priority: P0

## What it should do

Turn a project from a folder shortcut into a workspace. Store project instructions, attached folders/files, source status, and searchable project/thread content.

## How

Extend the `Project` type and Turso schema with instructions and source records. Add native folder/file selection and a Rust-owned index that respects `.gitignore`. Add debounced project and thread search APIs with cursor pagination; keep indexing and filesystem access outside the Svelte renderer.

## What it should look like

Give each project a detail header with name, path, instructions, and an `Add source` button. Show sources as removable rows with indexing status. Add a search box that groups results into project files, local chats, and matching thread messages, with a clear empty state.
