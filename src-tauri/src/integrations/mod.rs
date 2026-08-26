//! Integrations management: MCP servers, skills, and plugins.
//!
//! MCP servers live in `config.toml` under `[mcp_servers.<name>]`. Codex's own
//! config shape (see `codex-rs/config/src/mcp_types.rs`) supports two transports
//! — stdio (`command` + `args` + `env`) and streamable HTTP (`url`) — plus a
//! real `enabled` boolean that Codex honours when initializing servers. We edit
//! that file with `toml_edit` so comments and unrelated keys survive a
//! round-trip.
//!
//! Secrets never leave the native side: MCP `env` values are written to
//! `config.toml` but only key *names* are ever surfaced to the frontend. The
//! same goes for HTTP bearer-token env-var names.
//!
//! `config.toml` is only half the picture — it declares intent, not runtime
//! state. Whether a server actually started, what tools it exposes, and whether
//! the user is signed in all come from the running app-server; see
//! `app_server.rs`.

use serde::Serialize;

pub(crate) mod app_server;
pub(crate) mod commands;
mod config_doc;
pub(crate) mod skills_fs;

/// Everything the Integrations settings section needs in one call.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IntegrationsList {
    pub(crate) mcp_servers: Vec<McpServerSummary>,
    pub(crate) skills: Vec<SkillSummary>,
    pub(crate) plugins: Vec<PluginSummary>,
    /// Whether this build surfaces a real plugins mechanism. Currently `false`:
    /// Codex has an internal plugin-provided MCP concept but no user-facing
    /// install/enable surface, so we advertise the tab as unsupported.
    pub(crate) plugins_supported: bool,
}

/// Redacted view of one `[mcp_servers.<name>]` entry. Never carries secret
/// values — only the names of environment variables / bearer tokens.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpServerSummary {
    pub(crate) name: String,
    /// `"stdio"`, `"http"`, or `"unknown"` for malformed entries.
    pub(crate) transport: String,
    pub(crate) command: Option<String>,
    /// The stdio `args` array verbatim. Not secret, and the edit form needs the
    /// real values to round-trip a "Configure" without dropping them.
    pub(crate) args: Vec<String>,
    pub(crate) url: Option<String>,
    /// Names of `env` keys defined for a stdio server (values redacted).
    pub(crate) env_keys: Vec<String>,
    /// Name of the `bearer_token_env_var` for an HTTP server, if any.
    pub(crate) bearer_token_env_var: Option<String>,
    pub(crate) enabled: bool,
    /// MCP servers configured in `config.toml` are global to this Codex home.
    pub(crate) scope: String,
}

/// One skill as Codex resolves it. Unlike the directory scrape this replaced,
/// every field here comes from Codex itself, so descriptions and the real
/// enabled state are available.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SkillSummary {
    /// Possibly namespaced, e.g. `browser-use:browser`.
    pub name: String,
    pub path: String,
    /// `"user"` or `"system"`, as reported by `skills/list`.
    pub scope: String,
    /// The `SKILL.md` description — what the model matches against.
    pub description: Option<String>,
    pub enabled: bool,
    /// Presentation overrides from a plugin-provided skill's `interface` block.
    pub display_name: Option<String>,
    pub short_description: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginSummary {
    pub(crate) name: String,
    pub(crate) scope: String,
}

// Server health used to be probed by spawning the server ourselves and
// speaking raw MCP at it, which could only ever work for stdio transports and
// discarded everything but a tool count. `mcpServerStatus/list` reports the
// same handshake Codex already performed — for HTTP servers too — along with
// full tool schemas and auth state, so that code is gone.
