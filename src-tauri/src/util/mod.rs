//! Cross-cutting helpers with no domain knowledge of their own.
//!
//! Everything here is used by two or more domains; anything used by exactly one
//! belongs in that domain instead.

pub(crate) mod id;
pub(crate) mod json;
pub(crate) mod migration;
pub(crate) mod process;
pub(crate) mod time;
pub(crate) mod walk;
