//! Integrations state that only Codex can answer.
//!
//! `config.toml` says what MCP servers *should* exist; it cannot say whether
//! one started, what tools it exposes, or whether the user is signed in. Those
//! live in the running `codex app-server`, which we already hold a stdio
//! session to (see `crate::codex::session`). The methods below are the MCP and
//! skill half of that protocol:
//!
//! - `mcpServerStatus/list` — per-server `serverInfo`, `tools` (name → full
//!   schema), `resources`, and an `authStatus` of
//!   `unsupported | notLoggedIn | bearerToken | oAuth`.
//! - `mcpServer/oauth/login` — starts the OAuth dance. Codex owns the whole
//!   flow (callback port, credential storage) and reports back with an
//!   `mcpServer/oauthLogin/completed` notification, so we only kick it off.
//!   Streamable-HTTP servers only; stdio servers report `unsupported`.
//! - `config/mcpServer/reload` — re-reads `config.toml` without a restart.
//! - `skills/list` / `skills/config/write` — real skill metadata and the
//!   enable/disable hook, replacing the directory scrape this module used to do.
//!
//! These are deliberately thin: they forward the protocol's JSON straight to
//! the frontend rather than restating large, fast-moving structs in Rust. The
//! exception is `fetch_skills`, which the typed `IntegrationsList` needs.

use serde_json::Value;
use tauri::{AppHandle, State};

use super::SkillSummary;
use crate::codex::requests;
use crate::AppState;

#[tauri::command]
pub(crate) async fn list_mcp_server_status(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .send(&app, requests::mcp_server_status_list())
        .await
}

/// Start an OAuth login for one server. Resolves as soon as Codex has the flow
/// under way — completion arrives later as an `mcpServer/oauthLogin/completed`
/// notification, which the frontend listens for.
#[tauri::command]
pub(crate) async fn mcp_oauth_login(
    name: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .send(&app, requests::mcp_oauth_login(&name))
        .await
}

/// Make a running Codex pick up `config.toml` edits. Also called after every
/// mutation in `commands.rs`; without it the session keeps serving the servers
/// it started with until the app restarts.
#[tauri::command]
pub(crate) async fn reload_mcp_servers(
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    reload_mcp_config(&app, &ctx).await
}

/// The reload itself, callable from other commands (which hold `State` by
/// reference and so cannot invoke the command wrapper above).
pub(crate) async fn reload_mcp_config(
    app: &AppHandle,
    ctx: &crate::HomeContext,
) -> Result<Value, String> {
    ctx.session.send(app, requests::mcp_config_reload()).await
}

#[tauri::command]
pub(crate) async fn list_skills_for(
    cwds: Vec<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    ctx.session.send(&app, requests::skills_list(&cwds)).await
}

/// Enable or disable a skill by name. `skills/config/write` requires exactly
/// one of `name` or `path`; we always key by name.
#[tauri::command]
pub(crate) async fn set_skill_enabled(
    name: String,
    enabled: bool,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Value, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .send(&app, requests::skill_config_write(&name, enabled))
        .await
}

/// Skills for the typed `IntegrationsList`, deduped by name across the
/// requested cwds (a project skill and a user skill can share a name; the
/// first one Codex reports for a cwd wins, matching its own resolution order).
///
/// Best-effort: if Codex is unreachable or answers with a shape we don't
/// recognise, the Integrations view degrades to an empty skill list rather
/// than failing the whole call.
pub(crate) async fn fetch_skills(
    app: &AppHandle,
    ctx: &crate::HomeContext,
    cwds: Vec<String>,
    force_reload: bool,
) -> Vec<SkillSummary> {
    let req = if force_reload {
        requests::skills_list_force(&cwds)
    } else {
        requests::skills_list(&cwds)
    };
    let response = ctx.session.send(app, req).await;
    let Ok(value) = response else {
        return Vec::new();
    };
    parse_skills(&value)
}

/// Pure over the `skills/list` response so it can be unit-tested without a
/// running Codex.
pub fn parse_skills(response: &Value) -> Vec<SkillSummary> {
    let Some(groups) = response.get("data").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out: Vec<SkillSummary> = Vec::new();
    for group in groups {
        let Some(skills) = group.get("skills").and_then(Value::as_array) else {
            continue;
        };
        for skill in skills {
            let Some(name) = skill.get("name").and_then(Value::as_str) else {
                continue;
            };
            if out.iter().any(|existing| existing.name == name) {
                continue;
            }
            let interface = skill.get("interface");
            out.push(SkillSummary {
                name: name.to_string(),
                path: string_at(skill, "path").unwrap_or_default(),
                // Codex reports `user` / `system`; older builds said nothing.
                scope: string_at(skill, "scope").unwrap_or_else(|| "user".to_string()),
                description: string_at(skill, "description"),
                enabled: skill
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                display_name: interface.and_then(|i| string_at(i, "displayName")),
                short_description: interface.and_then(|i| string_at(i, "shortDescription")),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn string_at(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_skills_with_interface_metadata() {
        let response = json!({
            "data": [{
                "cwd": "/repo",
                "skills": [
                    {
                        "name": "browser-use:browser",
                        "description": "Browser automation for the in-app browser.",
                        "path": "/home/.codex/plugins/browser/SKILL.md",
                        "scope": "user",
                        "enabled": true,
                        "interface": {
                            "displayName": "Browser",
                            "shortDescription": "Open and control the in-app browser."
                        }
                    },
                    {
                        "name": "agents-sdk",
                        "description": "Build AI agents on Cloudflare Workers.",
                        "path": "/home/.codex/skills/agents-sdk/SKILL.md",
                        "scope": "system",
                        "enabled": false
                    }
                ],
                "errors": []
            }]
        });

        let skills = parse_skills(&response);
        let names: Vec<_> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["agents-sdk", "browser-use:browser"]);

        assert!(!skills[0].enabled);
        assert_eq!(skills[0].scope, "system");
        assert_eq!(skills[0].display_name, None);

        assert_eq!(skills[1].display_name.as_deref(), Some("Browser"));
        assert_eq!(
            skills[1].short_description.as_deref(),
            Some("Open and control the in-app browser.")
        );
    }

    #[test]
    fn dedupes_skills_reported_for_several_cwds() {
        let response = json!({
            "data": [
                { "cwd": "/a", "skills": [{ "name": "shared", "path": "/a/SKILL.md" }] },
                { "cwd": "/b", "skills": [{ "name": "shared", "path": "/b/SKILL.md" }] }
            ]
        });
        let skills = parse_skills(&response);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].path, "/a/SKILL.md");
        // `enabled` defaults to true when Codex omits it.
        assert!(skills[0].enabled);
    }

    #[test]
    fn malformed_responses_yield_no_skills() {
        assert!(parse_skills(&json!({})).is_empty());
        assert!(parse_skills(&json!({ "data": "nope" })).is_empty());
        // A skill without a name is unusable, so it is skipped rather than
        // surfaced with a blank label.
        assert!(parse_skills(&json!({ "data": [{ "skills": [{ "path": "/x" }] }] })).is_empty());
    }
}
