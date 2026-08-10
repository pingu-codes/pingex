# Contributing to Pingex

Thanks for helping improve Pingex. Make sure to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Local workflow

Use Deno for the frontend and Tauri commands; do not introduce npm lockfiles. Before proposing a change, run the checks relevant to it:

```sh
deno task check
deno task lint
deno task test
deno task rust:test
```

Run `deno task rust:fmt -- --check` and `deno task rust:lint -- -D warnings` for Rust changes. Add regression coverage for changed behavior, especially at app-server and persisted-state boundaries.

## Changes

- Keep the Codex CLI and `codex://` protocol integration compatible unless a change is explicitly coordinated.
- Treat paths, credentials, tokens, and user thread content as private. Never add them to fixtures, screenshots, commits, or bug reports.
- Preserve the copy-only Pingu Codex migration path until a separately announced compatibility policy replaces it.
- Keep macOS-specific behavior documented rather than silently claiming cross-platform support.

Use a concise problem statement, implementation summary, and verification results when opening a pull request. Repository hosting and issue-tracker links will be provided by the maintainer when publishing Pingex.

## Testing

Always make sure changes are tested against. If changing the frontend make sure there's both unit and integration tests.
