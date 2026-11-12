//! The review desk: pull requests, stacks and the worktrees they are checked
//! out in.
//!
//! `desk::ReviewDesk` is the service and the backend test seam; the Tauri
//! commands in `commands` are thin wrappers over it. GitHub sits behind the
//! `provider::GitHubProvider` port: `github::GhCli` is the production adapter
//! over the `gh` CLI, and `fake::FakeGitHub` backs tests. Nothing outside
//! `github` names `gh`.
//!
//! The wire types in `types` are provider-neutral. Unified patches become
//! hunks with anchors in `diff`; `feed` derives "since you last looked" from
//! two inbox snapshots; `pending` is the typed pending review and how it
//! becomes one GitHub review; `findings` is the contract an agent review's
//! final message follows, and its parser.

pub(crate) mod agent_commands;
pub(crate) mod agent_prompt;
pub(crate) mod agent_review;
pub(crate) mod checkout_commands;
pub(crate) mod checkout_types;
pub(crate) mod commands;
pub(crate) mod desk;
pub(crate) mod diff;
#[cfg(test)]
pub(crate) mod fake;
mod feed;
pub(crate) mod findings;
pub(crate) mod github;
pub(crate) mod pending;
pub(crate) mod provider;
pub(crate) mod repository;
pub(crate) mod stack_commands;
pub(crate) mod stack_types;
pub(crate) mod types;
pub(crate) mod worktree_context;
pub(crate) mod worktree_purpose;
