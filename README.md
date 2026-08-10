# Pingex

Pingex is a custom desktop frontend/GUI for [Codex](https://github.com/openai/codex) and its app-server interface powered by Tauri and Svelte. We aim to be a feature complete reimplementation of OpenAI's proprietary desktop app alongside adding some features!

Thanks to tauri and its rust backend I find Pingex is far less ram / CPU hungry and much quicker.

Pingex is not affiliated with, endorsed by, or sponsored by OpenAI. “OpenAI” and “Codex” are trademarks of their respective owners.

> This project is largely LLM generated so expect some slop (I wouldn't have the free time to have created this without my LLM overlords).

## Screenshots

![Thread overview](demo/screenshots/dark/07-thread-overview.png)

![Worktrees](demo/screenshots/dark/18-worktrees.png)

## Features

Pingex covers the core Codex desktop experience — threads with reasoning and command output, diff panels, reviews, slash commands, model and permission pickers, quick chat, and archived-thread search — and adds a few things on top of the basic app:

- **Better subagent control** — spawn and inspect subagents directly, with dedicated views for each subagent's thread and side questions.
- **Multi-project workspaces** — manage several projects side by side, with per-project details, workspace-wide search, and git worktree support for running parallel work in isolation.

## Requirements

- macOS (the current desktop handoff and quick-chat integrations use macOS APIs)
    - In the future we aim to support other platforms
- [Deno](https://deno.com/) 2+
- Rust toolchain
- A `codex` CLI on `PATH`, or a path configured in Pingex settings

## Development

```sh
deno task dev          # app against ~/.codex-personal
deno task frontend:dev # browser-only frontend preview
```

`PINGEX_CODEX_CLI_PATH` overrides the Codex binary for Pingex. The older `PINGU_CODEX_CLI_PATH` remains a compatibility fallback; prefer `CODEX_CLI_PATH` or the Pingex-specific variable for new setups.

## Verification

```sh
deno task check
deno task lint
deno task test
deno task rust:fmt -- --check
deno task rust:lint -- -D warnings
deno task rust:test
```

Browser preview coverage is available through `deno task test:e2e:install` followed by `deno task test:e2e`.

## Local build

```sh
deno task app:build
```

The local Tauri bundle is created under `src-tauri/target/release/bundle/`. It is unsigned, in the future we may sign it for developer use (if I can be bothered to pay the extortionate fee to Apple just to be able to sign binaries...)

## Migrating from Pingu Codex

On its first launch, Pingex copies valid Pingu Codex global settings and the per-`CODEX_HOME` local database into Pingex-owned locations. It never removes or overwrites the old files, so the old app remains a rollback option. Quit Pingu Codex before the first Pingex launch to ensure its database has flushed recent changes. Codex’s own `CODEX_HOME` data, CLI configuration, and `codex://` links are not renamed.

Existing `.pingu` workspace metadata is deliberately retained for compatibility with already-created workspaces.

## Contributing and security

Read [CONTRIBUTING.md](CONTRIBUTING.md), [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), and [SECURITY.md](SECURITY.md) before contributing or reporting a vulnerability. Support and issue-tracker locations will be selected when the maintainer publishes the repository.

## Dependency notices

Pingex’s source dependencies retain their own licenses. Before producing a distributed artifact, run the inventory procedure in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and include the resulting notices with that artifact.

## License

Pingex is licensed under the [MIT License](LICENSE).
