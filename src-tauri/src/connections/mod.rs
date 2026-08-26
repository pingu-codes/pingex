//! Remote connections management.
//!
//! The `codex app-server` remote-control API exposes `remoteControl/client/list`
//! and `remoteControl/client/revoke` (see the vendored protocol in
//! `codex-rs/app-server-protocol/src/protocol/v2/remote_control.rs`). There is no
//! protocol method for renaming a client or for a per-client "disconnect", so:
//!
//! * device display names and a cached `last_seen`/`paired_at` live in a
//!   frontend-only table (`store`);
//! * `rename_connection` writes to that table;
//! * `disconnect_connection` is a *safe* action that forgets the local record
//!   only — it never invalidates the credential, so an active device reappears
//!   on the next refresh (with its default name);
//! * `revoke_connection` is the *destructive* action: it calls
//!   `remoteControl/client/revoke` (idempotently) and then drops the local
//!   record.
//!
//! The device list merges the protocol-reported clients (source of truth for
//! health) with the stored records (custom names + devices recorded at pairing
//! claim time that have not yet surfaced in the protocol list). All state
//! changes are idempotent.

use serde::{Deserialize, Serialize};

pub(crate) mod commands;
mod protocol;
mod store;

/// Locally-persisted metadata for a paired device. Protocol tokens are never
/// stored here — only display metadata the frontend needs.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DeviceRecord {
    pub(crate) client_id: String,
    pub(crate) platform: Option<String>,
    /// User-chosen display name (overrides the protocol `displayName`).
    pub(crate) name: Option<String>,
    pub(crate) paired_at: i64,
    pub(crate) last_seen: Option<i64>,
    pub(crate) scope: Option<String>,
}

/// A client as reported by `remoteControl/client/list`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProtocolClient {
    pub(crate) client_id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) platform: Option<String>,
    pub(crate) device_model: Option<String>,
    pub(crate) app_version: Option<String>,
    pub(crate) last_seen_at: Option<i64>,
}

/// A merged connection returned to the frontend. Health is derived on the
/// frontend from `last_seen`; the backend stays presentation-free.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Connection {
    pub(crate) client_id: String,
    pub(crate) name: String,
    pub(crate) platform: Option<String>,
    pub(crate) device_model: Option<String>,
    pub(crate) app_version: Option<String>,
    pub(crate) paired_at: Option<i64>,
    pub(crate) last_seen: Option<i64>,
    pub(crate) scope: Option<String>,
    /// `"protocol"` when the live relay still reports this device, `"local"`
    /// when it is only known from a recorded pairing claim.
    pub(crate) source: String,
}
