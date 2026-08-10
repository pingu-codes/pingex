//! CLI <-> desktop handoff.
//!
//! A handoff opens the *same thread in the same state root*, not merely a
//! matching folder. This module owns:
//!
//! * Parsing the `codex://threads/<id>?path=&codexHome=&label=` deep link (and
//!   the `codex://threads/new` variant) into a structured value — pure functions
//!   with unit tests covering URL decoding, missing params, and bad ids (`url`).
//! * Resolving the requested home against the running home so the frontend can
//!   deliberately switch, warn, or error rather than silently falling back
//!   (`deeplink`).
//! * Building the reproducible `CODEX_HOME=… codex resume <id> --cd <cwd>`
//!   command with proper POSIX shell quoting (`url`, pure + unit-tested).
//! * Handing that command to the OS — clipboard or Terminal.app (`terminal`).

pub(crate) mod commands;
mod deeplink;
mod terminal;
mod url;

pub(crate) use deeplink::handle_deep_link_url;
