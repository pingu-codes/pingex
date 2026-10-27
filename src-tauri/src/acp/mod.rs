//! The generic ACP driver (Agent Client Protocol, JSON-RPC over stdio): one
//! driver for every harness that speaks ACP and has no native driver
//! (`docs/adr/0002-harness-interface.md`). What differs per agent lives in
//! its [`spec::AcpSpec`]; maki is the first.
//!
//! Protocol notes in `docs/research/acp-neutral-model.md`.

pub(crate) mod driver;
pub mod maki;
pub mod permissions;
pub(crate) mod rpc;
pub mod session;
pub mod spec;
pub(crate) mod translate;
