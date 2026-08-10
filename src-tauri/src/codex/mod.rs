//! The client side of the Codex app-server: locating the CLI, running one
//! long-lived child process, and the remote-pairing handshake that lets a phone
//! drive that same session.

pub(crate) mod binary;
pub(crate) mod child;
pub(crate) mod journal;
pub(crate) mod pairing;
pub(crate) mod session;
pub(crate) mod wire;

pub(crate) use session::CodexSession;
