//! The review desk: pull requests, stacks and the checkouts they need.
//!
//! `desk::ReviewDesk` is the service and the backend test seam; the Tauri
//! commands in `commands` are thin wrappers over it. GitHub sits behind the
//! `provider::GitHubProvider` port: `github::GhCli` is the production adapter
//! over the `gh` CLI, and `fake::FakeGitHub` backs tests. Nothing outside
//! `github` names `gh`.
//!
//! The wire types in `types` are provider-neutral. Unified patches become
//! hunks with anchors in `diff`; `feed` derives "since you last looked" from
//! two inbox snapshots; `requirements` is the table of the checkout step each
//! desk action needs; `pending` is the typed pending review and how it
//! becomes one GitHub review.

pub(crate) mod commands;
pub(crate) mod desk;
pub(crate) mod diff;
#[cfg(test)]
pub(crate) mod fake;
mod feed;
pub(crate) mod github;
pub(crate) mod pending;
pub(crate) mod provider;
pub(crate) mod repository;
pub(crate) mod requirements;
pub(crate) mod types;
