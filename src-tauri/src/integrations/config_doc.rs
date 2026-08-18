//! Reading and editing `[mcp_servers.*]` in `config.toml`.
//!
//! Edited with `toml_edit` so comments and unrelated keys survive a round-trip.
//! Secrets never leave the native side: `env` values are written to the file but
//! only key *names* are ever surfaced, and reconfiguring a server without
//! supplying new `env` preserves what is already there.

use std::collections::BTreeMap;
use std::path::Path;
use toml_edit::{Array, DocumentMut, Item, Table};

use super::McpServerSummary;
use crate::settings::codex_config::{read_doc, write_doc};

pub(crate) fn validate_server_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Server name cannot be empty".to_string());
    }
    if trimmed != name {
        return Err("Server name cannot have leading or trailing whitespace".to_string());
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err("Server name may only contain letters, numbers, '-', '_', and '.'".to_string());
    }
    Ok(())
}

/// Insert or replace a stdio server entry. `env` values are written verbatim to
/// disk but never echoed back to the caller.
///
/// `env_keys` is the caller's full desired set of env variable names, which is
/// what makes secret handling work in an edit form that never sees the values:
/// a key present in `env_keys` but absent from `env` keeps whatever is already
/// on disk, and a key in neither is dropped. `None` means "leave the existing
/// env table exactly as it is" — the add path, and any caller that does not
/// model env at all.
pub(crate) fn upsert_stdio_server(
    doc: &mut DocumentMut,
    name: &str,
    command: &str,
    args: &[String],
    env: &BTreeMap<String, String>,
    env_keys: Option<&[String]>,
) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("Command cannot be empty".to_string());
    }
    let servers = servers_table(doc)?;

    let existing = servers.get(name).and_then(Item::as_table_like);
    let existing_env: BTreeMap<String, Item> = existing
        .and_then(|table| table.get("env"))
        .and_then(Item::as_table_like)
        .map(|env| {
            env.iter()
                .map(|(key, item)| (key.to_string(), item.clone()))
                .collect()
        })
        .unwrap_or_default();
    // Carry over the enabled flag so re-saving does not silently re-enable a
    // disabled server.
    let preserved_enabled = existing.and_then(|table| table.get("enabled")).cloned();

    let mut table = Table::new();
    table.insert("command", toml_edit::value(command));
    if !args.is_empty() {
        let mut array = Array::new();
        for arg in args {
            array.push(arg.as_str());
        }
        table.insert("args", toml_edit::value(array));
    }

    let mut env_table = toml_edit::Table::new();
    match env_keys {
        Some(keys) => {
            for key in keys {
                match env.get(key) {
                    // A freshly typed value replaces what is on disk.
                    Some(value) => {
                        env_table.insert(key, toml_edit::value(value.as_str()));
                    }
                    // Left blank in the form: keep the stored secret, if any.
                    None => {
                        if let Some(item) = existing_env.get(key) {
                            env_table.insert(key, item.clone());
                        }
                    }
                }
            }
        }
        None => {
            for (key, item) in &existing_env {
                env_table.insert(key, item.clone());
            }
            for (key, value) in env {
                env_table.insert(key, toml_edit::value(value.as_str()));
            }
        }
    }
    if !env_table.is_empty() {
        table.insert("env", Item::Table(env_table));
    }

    if let Some(enabled_item) = preserved_enabled {
        table.insert("enabled", enabled_item);
    }
    servers.insert(name, Item::Table(table));
    Ok(())
}

pub(crate) fn upsert_http_server(
    doc: &mut DocumentMut,
    name: &str,
    url: &str,
    bearer_token_env_var: Option<&str>,
) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("URL cannot be empty".to_string());
    }
    let servers = servers_table(doc)?;
    let preserved_enabled = servers
        .get(name)
        .and_then(Item::as_table_like)
        .and_then(|table| table.get("enabled"))
        .cloned();

    let mut table = Table::new();
    table.insert("url", toml_edit::value(url));
    if let Some(env_var) = bearer_token_env_var
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        table.insert("bearer_token_env_var", toml_edit::value(env_var));
    }
    if let Some(enabled_item) = preserved_enabled {
        table.insert("enabled", enabled_item);
    }
    servers.insert(name, Item::Table(table));
    Ok(())
}

/// Move an existing entry to a new key, preserving everything under it.
///
/// Renaming is a key move rather than a delete-and-recreate so `env` secrets and
/// any keys this app does not model (Codex gains config options faster than this
/// UI does) survive the edit.
pub(crate) fn rename_server_in_doc(
    doc: &mut DocumentMut,
    from: &str,
    to: &str,
) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    let servers = servers_table(doc)?;
    if servers.contains_key(to) {
        return Err(format!("An MCP server named '{to}' already exists"));
    }
    let existing = servers
        .remove(from)
        .ok_or_else(|| format!("No MCP server named '{from}'"))?;
    servers.insert(to, existing);
    Ok(())
}

/// The `[mcp_servers]` table, created (implicitly) if absent.
fn servers_table(doc: &mut DocumentMut) -> Result<&mut Table, String> {
    let servers = doc
        .entry("mcp_servers")
        .or_insert_with(|| Item::Table(Table::new()));
    let servers = servers
        .as_table_mut()
        .ok_or_else(|| "mcp_servers is not a table".to_string())?;
    servers.set_implicit(true);
    Ok(servers)
}

pub(crate) fn remove_server_from_doc(doc: &mut DocumentMut, name: &str) -> Result<(), String> {
    let removed = doc
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
        .map(|servers| servers.remove(name).is_some())
        .unwrap_or(false);
    if !removed {
        return Err(format!("No MCP server named '{name}'"));
    }
    Ok(())
}

pub(crate) fn set_enabled_in_doc(
    doc: &mut DocumentMut,
    name: &str,
    enabled: bool,
) -> Result<(), String> {
    let server = doc
        .get_mut("mcp_servers")
        .and_then(Item::as_table_mut)
        .and_then(|servers| servers.get_mut(name))
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| format!("No MCP server named '{name}'"))?;
    server.insert("enabled", toml_edit::value(enabled));
    Ok(())
}

/// Load the document for a home, ready to edit.
pub(crate) fn load(codex_home: &Path) -> Result<DocumentMut, String> {
    read_doc(codex_home)
}

/// Persist an edited document back to the home.
pub(crate) fn save(codex_home: &Path, doc: &DocumentMut) -> Result<(), String> {
    write_doc(codex_home, doc)
}

/// Extract redacted summaries for every `[mcp_servers.*]` entry, sorted by name.
///
/// Pure over a parsed document so it can be unit-tested without touching disk.
pub(crate) fn summarize_mcp_servers(doc: &DocumentMut) -> Vec<McpServerSummary> {
    let Some(servers) = doc.get("mcp_servers").and_then(Item::as_table) else {
        return Vec::new();
    };
    let mut out: Vec<McpServerSummary> = servers
        .iter()
        .filter_map(|(name, item)| item.as_table_like().map(|table| (name, table)))
        .map(|(name, table)| {
            let command = table
                .get("command")
                .and_then(Item::as_str)
                .map(str::to_string);
            let url = table.get("url").and_then(Item::as_str).map(str::to_string);
            let args = table
                .get("args")
                .and_then(Item::as_array)
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let env_keys = table
                .get("env")
                .and_then(Item::as_table_like)
                .map(|env| env.iter().map(|(key, _)| key.to_string()).collect())
                .unwrap_or_default();
            let bearer_token_env_var = table
                .get("bearer_token_env_var")
                .and_then(Item::as_str)
                .map(str::to_string);
            // `enabled` defaults to true, matching Codex's `default_enabled`.
            let enabled = table.get("enabled").and_then(Item::as_bool).unwrap_or(true);
            let transport = if command.is_some() {
                "stdio"
            } else if url.is_some() {
                "http"
            } else {
                "unknown"
            };
            McpServerSummary {
                name: name.to_string(),
                transport: transport.to_string(),
                command,
                args,
                url,
                env_keys,
                bearer_token_env_var,
                enabled,
                scope: "global".to_string(),
            }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml_edit::Value as TomlValue;

    fn doc(text: &str) -> DocumentMut {
        text.parse::<DocumentMut>().unwrap()
    }

    /// Read a raw `env` value straight out of the document. Summaries redact
    /// these by design, so tests that assert secrets survived an edit have to
    /// look at the file itself.
    fn env_value(doc: &DocumentMut, server: &str, key: &str) -> Option<String> {
        doc.get("mcp_servers")
            .and_then(Item::as_table)
            .and_then(|servers| servers.get(server))
            .and_then(Item::as_table_like)
            .and_then(|table| table.get("env"))
            .and_then(Item::as_table_like)
            .and_then(|env| env.get(key))
            .and_then(|item| item.as_value())
            .and_then(TomlValue::as_str)
            .map(str::to_string)
    }

    #[test]
    fn summarizes_stdio_and_http_servers_sorted() {
        let document = doc(r#"
[mcp_servers.zed]
command = "npx"
args = ["-y", "server-zed"]
env = { API_KEY = "secret", REGION = "us" }

[mcp_servers.aleph]
url = "https://example.com/mcp"
bearer_token_env_var = "ALEPH_TOKEN"
enabled = false
"#);
        let servers = summarize_mcp_servers(&document);
        assert_eq!(servers.len(), 2);
        // Sorted by name: aleph before zed.
        assert_eq!(servers[0].name, "aleph");
        assert_eq!(servers[0].transport, "http");
        assert_eq!(servers[0].url.as_deref(), Some("https://example.com/mcp"));
        assert_eq!(
            servers[0].bearer_token_env_var.as_deref(),
            Some("ALEPH_TOKEN")
        );
        assert!(!servers[0].enabled);

        assert_eq!(servers[1].name, "zed");
        assert_eq!(servers[1].transport, "stdio");
        assert_eq!(servers[1].command.as_deref(), Some("npx"));
        assert_eq!(
            servers[1].args,
            vec!["-y".to_string(), "server-zed".to_string()]
        );
        assert!(servers[1].enabled, "missing enabled key defaults to true");
    }
    #[test]
    fn summary_redacts_env_values_exposing_only_names() {
        let document = doc(r#"
[mcp_servers.secretive]
command = "run"
env = { TOKEN = "super-secret", OTHER = "also-secret" }
"#);
        let servers = summarize_mcp_servers(&document);
        let mut keys = servers[0].env_keys.clone();
        keys.sort();
        assert_eq!(keys, vec!["OTHER".to_string(), "TOKEN".to_string()]);
        // Serialized output must never contain the secret values.
        let serialized = serde_json::to_string(&servers[0]).unwrap();
        assert!(!serialized.contains("super-secret"));
        assert!(!serialized.contains("also-secret"));
    }
    #[test]
    fn upsert_writes_env_and_round_trips_preserving_comments() {
        let mut document = doc(r#"
# Keep me
model = "gpt-5"

[mcp_servers.existing]
command = "old"
"#);
        let mut env = BTreeMap::new();
        env.insert("API_KEY".to_string(), "value123".to_string());
        upsert_stdio_server(
            &mut document,
            "fresh",
            "npx",
            &["-y".to_string(), "pkg".to_string()],
            &env,
            None,
        )
        .unwrap();
        let rendered = document.to_string();
        assert!(rendered.contains("# Keep me"), "comment survived");
        assert!(rendered.contains("[mcp_servers.existing]"), "existing kept");

        // Re-parse and confirm the new server reads back with redaction.
        let reparsed = doc(&rendered);
        let servers = summarize_mcp_servers(&reparsed);
        let fresh = servers.iter().find(|s| s.name == "fresh").unwrap();
        assert_eq!(fresh.command.as_deref(), Some("npx"));
        assert_eq!(fresh.args.len(), 2);
        assert_eq!(fresh.env_keys, vec!["API_KEY".to_string()]);
        // But the real value is still on disk for Codex to launch with.
        assert_eq!(
            env_value(&reparsed, "fresh", "API_KEY").as_deref(),
            Some("value123")
        );
    }
    #[test]
    fn upsert_replaces_existing_entry() {
        let mut document = doc(r#"
[mcp_servers.dup]
command = "old"
args = ["a", "b", "c"]
"#);
        upsert_stdio_server(&mut document, "dup", "new", &[], &BTreeMap::new(), None).unwrap();
        let servers = summarize_mcp_servers(&document);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].command.as_deref(), Some("new"));
        assert_eq!(servers[0].args.len(), 0);
    }
    #[test]
    fn reconfigure_without_new_env_preserves_secrets_and_enabled() {
        let mut document = doc(r#"
[mcp_servers.keep]
command = "old"
enabled = false
env = { SECRET = "keep-me" }
"#);
        // Configure with only a new command + args, no env supplied.
        upsert_stdio_server(
            &mut document,
            "keep",
            "new",
            &["--flag".to_string()],
            &BTreeMap::new(),
            None,
        )
        .unwrap();
        let summary = &summarize_mcp_servers(&document)[0];
        assert_eq!(summary.command.as_deref(), Some("new"));
        assert_eq!(summary.args.len(), 1);
        // Secret and disabled state survived the edit.
        assert_eq!(
            env_value(&document, "keep", "SECRET").as_deref(),
            Some("keep-me")
        );
        assert!(!summary.enabled);
    }
    #[test]
    fn upsert_rejects_empty_command() {
        let mut document = doc("");
        let error =
            upsert_stdio_server(&mut document, "x", "  ", &[], &BTreeMap::new(), None).unwrap_err();
        assert!(error.contains("Command"));
    }
    #[test]
    fn env_keys_prune_removed_vars_while_keeping_untouched_secrets() {
        let mut document = doc(r#"
[mcp_servers.keep]
command = "npx"
env = { KEPT = "old-secret", DROPPED = "gone", REPLACED = "stale" }
"#);
        let mut env = BTreeMap::new();
        env.insert("REPLACED".to_string(), "fresh".to_string());
        // The form still shows KEPT and REPLACED; the user deleted DROPPED and
        // only typed a new value for REPLACED.
        upsert_stdio_server(
            &mut document,
            "keep",
            "npx",
            &[],
            &env,
            Some(&["KEPT".to_string(), "REPLACED".to_string()]),
        )
        .unwrap();
        assert_eq!(
            summarize_mcp_servers(&document)[0].env_keys,
            vec!["KEPT".to_string(), "REPLACED".to_string()]
        );
        assert_eq!(
            env_value(&document, "keep", "KEPT").as_deref(),
            Some("old-secret")
        );
        assert_eq!(
            env_value(&document, "keep", "REPLACED").as_deref(),
            Some("fresh")
        );
        assert_eq!(env_value(&document, "keep", "DROPPED"), None);

        // Clearing every key removes the env table entirely.
        upsert_stdio_server(
            &mut document,
            "keep",
            "npx",
            &[],
            &BTreeMap::new(),
            Some(&[]),
        )
        .unwrap();
        assert!(summarize_mcp_servers(&document)[0].env_keys.is_empty());
    }

    #[test]
    fn upsert_http_preserves_enabled_and_clears_blank_token() {
        let mut document = doc(r#"
[mcp_servers.remote]
url = "https://old"
bearer_token_env_var = "OLD_TOKEN"
enabled = false
"#);
        upsert_http_server(&mut document, "remote", "https://new", Some("NEW_TOKEN")).unwrap();
        let summary = &summarize_mcp_servers(&document)[0];
        assert_eq!(summary.transport, "http");
        assert_eq!(summary.url.as_deref(), Some("https://new"));
        assert_eq!(summary.bearer_token_env_var.as_deref(), Some("NEW_TOKEN"));
        assert!(!summary.enabled, "disabled state survived the edit");

        upsert_http_server(&mut document, "remote", "https://new", Some("  ")).unwrap();
        assert_eq!(
            summarize_mcp_servers(&document)[0].bearer_token_env_var,
            None
        );
    }

    #[test]
    fn upsert_http_rejects_empty_url() {
        let mut document = doc("");
        assert!(upsert_http_server(&mut document, "x", " ", None).is_err());
    }

    #[test]
    fn rename_moves_entry_keeping_secrets_and_rejects_collisions() {
        let mut document = doc(r#"
[mcp_servers.old]
command = "npx"
args = ["-y", "pkg"]
env = { SECRET = "keep-me" }

[mcp_servers.taken]
command = "x"
"#);
        assert!(rename_server_in_doc(&mut document, "old", "taken").is_err());
        assert!(rename_server_in_doc(&mut document, "absent", "fresh").is_err());
        rename_server_in_doc(&mut document, "old", "new").unwrap();

        let names: Vec<String> = summarize_mcp_servers(&document)
            .into_iter()
            .map(|server| server.name)
            .collect();
        assert_eq!(names, vec!["new".to_string(), "taken".to_string()]);
        assert_eq!(
            env_value(&document, "new", "SECRET").as_deref(),
            Some("keep-me")
        );
        // Renaming to the same key is a no-op, not a "already exists" error.
        rename_server_in_doc(&mut document, "new", "new").unwrap();
    }

    #[test]
    fn remove_reports_missing_and_deletes_present() {
        let mut document = doc(r#"
[mcp_servers.gone]
command = "x"
"#);
        assert!(remove_server_from_doc(&mut document, "absent").is_err());
        remove_server_from_doc(&mut document, "gone").unwrap();
        assert!(summarize_mcp_servers(&document).is_empty());
    }
    #[test]
    fn set_enabled_toggles_flag() {
        let mut document = doc(r#"
[mcp_servers.toggle]
command = "x"
"#);
        set_enabled_in_doc(&mut document, "toggle", false).unwrap();
        assert!(!summarize_mcp_servers(&document)[0].enabled);
        set_enabled_in_doc(&mut document, "toggle", true).unwrap();
        assert!(summarize_mcp_servers(&document)[0].enabled);
        assert!(set_enabled_in_doc(&mut document, "missing", false).is_err());
    }
    #[test]
    fn validates_server_names() {
        assert!(validate_server_name("good-name_1.2").is_ok());
        assert!(validate_server_name("").is_err());
        assert!(validate_server_name(" spaced").is_err());
        assert!(validate_server_name("has space").is_err());
        assert!(validate_server_name("bad/slash").is_err());
    }
    #[test]
    fn http_servers_are_summarized_with_their_transport() {
        let document = doc(r#"
[mcp_servers.http]
url = "https://x"
bearer_token_env_var = "TOKEN"
"#);
        let summary = &summarize_mcp_servers(&document)[0];
        assert_eq!(summary.transport, "http");
        assert_eq!(summary.command, None);
        assert_eq!(summary.bearer_token_env_var.as_deref(), Some("TOKEN"));
    }
    #[test]
    fn missing_config_is_empty_not_error() {
        let dir = tempfile::tempdir().unwrap();
        let document = read_doc(dir.path()).unwrap();
        assert!(summarize_mcp_servers(&document).is_empty());
    }
}
