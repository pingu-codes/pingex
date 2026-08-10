//! Provider-neutral pull-request review service.
//!
//! Read-only inspection and review actions go through this module. The first
//! (and currently only) adapter is GitHub, backed by the installed `gh` CLI. As
//! with the native Git service, every invocation uses an argument array (never a
//! shell string), a bounded timeout, and redacted errors so raw stderr never
//! leaks. Absence of `gh` or a missing login is reported as an actionable state
//! rather than a hard failure.
//!
//! The wire types (`PrSummary`, `PrFile`, `DiffHunk`, `PrComment`, ...) are
//! deliberately provider-neutral; a future adapter (GitLab, Gitea) would map its
//! own API onto the same shapes so the frontend never changes. Pure parsing and
//! mapping is split in two — `gh` JSON -> types in `parse`, unified patch ->
//! hunks with anchors in `diff` — and both are unit tested with fixtures; the
//! network is never touched in tests.

mod actions;
pub(crate) mod commands;
mod diff;
mod gh;
mod parse;
mod queries;
mod types;
