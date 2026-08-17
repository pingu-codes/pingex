//! Threads: reading them, running turns in them, and managing their lifecycle.
//!
//! The app-server owns thread state; this module owns the projections and the
//! local additions Codex has no concept of (persisted questions, side questions,
//! the search index).

pub(crate) mod autoname;
pub(crate) mod lifecycle;
pub(crate) mod queue;
pub(crate) mod read;
pub(crate) mod search;
pub(crate) mod side_questions;
pub(crate) mod subagents;
pub(crate) mod turn;
