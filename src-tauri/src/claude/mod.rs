//! The Claude Code driver: one `claude -p` process per active thread, speaking
//! stream-json, translated into the neutral event model.
//!
//! Protocol notes in `docs/research/claude-code-stdio.md`; the design in
//! `features/13-harnesses.md`.

pub(crate) mod child;
pub(crate) mod driver;
pub(crate) mod permissions;
pub(crate) mod tools;
pub(crate) mod translate;

pub(crate) use driver::ClaudeDriver;
