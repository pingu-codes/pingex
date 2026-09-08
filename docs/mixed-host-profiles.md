# Windows and WSL projects in one window

Add a project using **Add project**, choose This computer or an installed WSL
distribution, and enter a directory on that Host. Browsing a WSL share also
selects its distribution. The sidebar labels WSL projects with the distribution.
Windows `C:\repo` and WSL `/mnt/c/repo` remain separate project entries even
when they access the same files.

Each Host has separate Codex and Claude Code defaults. Settings lists the
Profile's Homes and can add a named Home for another configuration or account.
The composer selects a harness and Home for a new conversation. Existing
conversations retain their Home when defaults or the selected project change.
Multi-project workspaces accept members from one Host and can use either
harness on that Host.

## Storage and routing

Each previously separate Codex Home opens its own local Profile. The Profile
directory is under the OS application data directory, in
`pingex/profiles/<stable-id>/`. Its root `pingex.db` contains the Home registry,
project registrations, shared sidebar organization and imported history.
Additional Homes have local conversation caches in `homes/<home-id>/pingex.db`.
The original Claude Home retains access to its imported journal.

Migration copies schema and rows through a database read transaction, including
committed WAL data. The data and completion marker commit together. Reopening
does not re-import over Profile edits. The original database remains separate.
This keeps existing account separation and avoids running SQLite over WSL shares.

Frontend conversation references pair the Home key with the native harness ID.
Project references retain Host-qualified paths. Each generated command carries
an optional Home route; Rust validates Profile membership and captures the Home
context before running the handler. Events and approval requests receive the
same qualification before reaching frontend stores. Model and account caches
are per Home. A disconnect affects only its Home's conversations.

The Profile registry and per-Home caches preserve the existing storage interfaces.
They are separate databases, so project metadata updates across Homes are not a
single transaction. Workspace metadata is copied to a second Home on the same
Host before starting or moving a conversation there.

## Verification

Routing tests cover colliding native thread and approval IDs, navigation changes,
both harnesses, and Windows and WSL paths. Rust tests cover Profile membership,
captured contexts and repeatable imports that preserve the source database.

Set `PINGEX_E2E_MIXED_HOSTS` to an installed distribution and run
`cargo test --test live_mixed_hosts -- --nocapture` from `src-tauri` to launch
Codex and Claude Code on Windows and WSL concurrently with `--version`.
This smoke test does not send model turns.
