//! Everything configurable, across the two places configuration lives.
//!
//! `prefs` is Pingex's own file outside CODEX_HOME (it may itself relocate
//! CODEX_HOME, so it cannot live inside one). `codex_config` and `overview`
//! read and write Codex's `config.toml` in the active home. `runtime` resolves
//! which home and CLI are active in the first place.

pub(crate) mod codex_config;
pub(crate) mod commands;
pub(crate) mod overview;
pub(crate) mod prefs;
pub(crate) mod runtime;
