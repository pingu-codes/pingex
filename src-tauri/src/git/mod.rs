//! Native Git service.
//!
//! The frontend never shells out to Git; every Git interaction goes through
//! these commands. Read-only inspection runs the installed `git` executable
//! with an explicit `-C <dir>`, argument arrays (never a shell string), a short
//! timeout, and bounded, structured output. Errors are redacted so raw stderr
//! never leaks paths beyond the repository the caller already knows about.
//!
//! Mutating operations (`worktree add/remove/prune/lock/unlock`) are serialized
//! per Git *common directory* so two concurrent mutations against the same
//! repository can never interleave.

mod branches;
pub(crate) mod commands;
mod commits;
mod run;

/// Exposed so the review service's local-diff mode reuses the same runner
/// rather than shelling out to `git` a second way.
pub(crate) use run::run_git;

mod status;
mod types;
mod worktrees;
