//! The project tree the sidebar shows, and the bootstrap payload that fills it.
//!
//! A "project" is whatever can hold threads: a folder the user added, a
//! Codex-managed worktree discovered under the home, or a workspace hub. This
//! module owns the projection from raw app-server threads into that tree.

pub(crate) mod bootstrap;
pub(crate) mod commands;
pub(crate) mod server;
pub(crate) mod summary;
pub(crate) mod types;
pub(crate) mod worktrees;

pub(crate) use bootstrap::{bootstrap_cached, bootstrap_inner};
pub(crate) use summary::{strip_mention_markup, thread_search_row, thread_summary_from};
pub(crate) use types::{BootstrapData, ThreadSummary};
