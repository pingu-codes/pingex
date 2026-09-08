//! Integration preferences are resolved by Codex; only the selected config layer is edited.
use super::{app_server::parse_skill_entries, IntegrationsList, McpServerSummary, PluginSummary};
use crate::settings::codex_config::read_doc;
use crate::{
    codex::{compat::Feature, requests},
    AppState, HomeContext,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Write;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use tauri::{AppHandle, State};
use toml_edit::DocumentMut;

#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IntegrationSetting {
    pub inherited_enabled: bool,
    pub override_enabled: Option<bool>,
    pub plugin_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Copy, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum IntegrationKind {
    Mcp,
    Plugin,
}

impl IntegrationKind {
    fn table(self) -> &'static str {
        match self {
            Self::Mcp => "mcp_servers",
            Self::Plugin => "plugins",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Mcp => "mcp",
            Self::Plugin => "plugin",
        }
    }
}

fn merge(target: &mut Value, source: &Value) {
    if let (Some(to), Some(from)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in from {
            merge(to.entry(key).or_insert(Value::Null), value);
        }
    } else {
        *target = source.clone();
    }
}

fn enabled(config: &Value, table: &str, id: &str) -> Option<bool> {
    config.get(table)?.get(id)?.get("enabled")?.as_bool()
}

fn project_dir(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() || !path.is_dir() {
        return Err("Project must be an existing absolute directory.".into());
    }
    Ok(path.join(".codex"))
}

fn as_json(doc: &DocumentMut) -> Result<Value, String> {
    let parsed: toml::Value =
        toml::from_str(&doc.to_string()).map_err(|e| format!("Invalid integration config: {e}"))?;
    serde_json::to_value(parsed).map_err(|e| e.to_string())
}

fn mcp_summaries(config: &Value) -> Vec<McpServerSummary> {
    config["mcp_servers"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(name, value)| McpServerSummary {
            name: name.clone(),
            transport: if value["command"].is_string() {
                "stdio"
            } else if value["url"].is_string() {
                "http"
            } else {
                "unknown"
            }
            .into(),
            command: value["command"].as_str().map(str::to_owned),
            args: value["args"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
            url: value["url"].as_str().map(str::to_owned),
            env_keys: value["env"]
                .as_object()
                .into_iter()
                .flat_map(|v| v.keys().cloned())
                .collect(),
            bearer_token_env_var: value["bearer_token_env_var"].as_str().map(str::to_owned),
            enabled: value["enabled"].as_bool().unwrap_or(true),
            scope: "effective".into(),
        })
        .collect()
}

/// The protocol sends layers highest first. Merge lowest first, excluding the edited layer
/// for the inherited value. Never use a disabled project layer as effective config.
fn inherited_config(response: &Value, selected: &Path) -> Value {
    let mut inherited = json!({});
    for layer in response["layers"].as_array().into_iter().flatten().rev() {
        let name = &layer["name"];
        let is_selected = name["dotCodexFolder"].as_str().map(Path::new) == Some(selected)
            || name["file"].as_str().map(Path::new) == Some(selected.join("config.toml").as_path());
        if !is_selected && layer["disabledReason"].is_null() {
            merge(&mut inherited, &layer["config"]);
        }
    }
    inherited
}

fn parse_plugins(value: &Value) -> Vec<PluginSummary> {
    let mut result = BTreeMap::new();
    for marketplace in value["marketplaces"].as_array().into_iter().flatten() {
        for plugin in marketplace["plugins"].as_array().into_iter().flatten() {
            if plugin["installed"].as_bool() != Some(true) {
                continue;
            }
            let (Some(id), Some(name)) = (plugin["id"].as_str(), plugin["name"].as_str()) else {
                continue;
            };
            result
                .entry(id.to_owned())
                .or_insert_with(|| PluginSummary {
                    id: id.into(),
                    name: plugin["interface"]["displayName"]
                        .as_str()
                        .unwrap_or(name)
                        .into(),
                    scope: marketplace["name"].as_str().unwrap_or("installed").into(),
                    description: plugin["interface"]["shortDescription"]
                        .as_str()
                        .map(str::to_owned),
                    enabled: plugin["enabled"].as_bool().unwrap_or(false),
                });
        }
    }
    result.into_values().collect()
}

pub(super) async fn read(
    app: &AppHandle,
    ctx: &HomeContext,
    cwds: Vec<String>,
    force: bool,
) -> Result<IntegrationsList, String> {
    if cwds.len() > 1 {
        return Err("Select one project to manage its integrations.".into());
    }
    let home = ctx.runtime().codex_home;
    let project = cwds.first();
    let selected = match project {
        Some(path) => project_dir(path)?,
        None => home.clone(),
    };
    let cwd = project
        .cloned()
        .unwrap_or_else(|| home.to_string_lossy().into_owned());
    let local = as_json(&read_doc(&selected)?)?;
    let config = ctx
        .session
        .send(app, requests::integration_config(Some(&cwd)))
        .await?;
    if !config["layers"].is_array() || !config["config"].is_object() {
        return Err("Codex did not return configuration layers.".into());
    }
    let inherited = inherited_config(&config, &selected);
    let mut errors = Vec::new();
    for layer in config["layers"].as_array().into_iter().flatten() {
        if let Some(reason) = layer["disabledReason"].as_str() {
            errors.push(format!("Configuration ignored: {reason}"));
        }
    }
    let req = if force {
        requests::skills_list_force(&[cwd.clone()])
    } else {
        requests::skills_list(&[cwd.clone()])
    };
    // A failed request must not replace a previously loaded list with an empty success.
    let raw_skills = ctx.session.send(app, req).await?;
    for group in raw_skills["data"].as_array().into_iter().flatten() {
        for error in group["errors"].as_array().into_iter().flatten() {
            if let Some(message) = error["message"].as_str() {
                errors.push(format!("Skills: {message}"));
            }
        }
    }
    if !raw_skills["data"].is_array() {
        return Err("Codex returned an invalid skill inventory.".into());
    }
    let mut skills = parse_skill_entries(&raw_skills, true);
    let plugin_result = ctx
        .session
        .send_gated(
            app,
            Feature::INSTALLED_PLUGINS,
            requests::installed_plugins(&cwds),
            |_| None,
        )
        .await;
    let (plugins, plugins_supported, catalog) = match plugin_result {
        Ok(raw) => {
            if !raw["marketplaces"].is_array() {
                return Err("Codex returned an invalid plugin inventory.".into());
            }
            for error in raw["marketplaceLoadErrors"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(message) = error["message"].as_str() {
                    errors.push(format!("Plugins: {message}"));
                }
            }
            (parse_plugins(&raw), true, raw)
        }
        Err(e) if e.starts_with(Feature::INSTALLED_PLUGINS.error_prefix) => {
            (Vec::new(), false, Value::Null)
        }
        Err(e) => return Err(e),
    };
    let mut mcp_servers = mcp_summaries(&config["config"]);
    for server in &mut mcp_servers {
        server.scope = if local["mcp_servers"][&server.name]["command"].is_string()
            || local["mcp_servers"][&server.name]["url"].is_string()
        {
            "user"
        } else {
            "inherited"
        }
        .into();
    }
    let mut settings = BTreeMap::new();
    for (kind, id) in mcp_servers
        .iter()
        .map(|s| (IntegrationKind::Mcp, s.name.as_str()))
        .chain(
            plugins
                .iter()
                .map(|p| (IntegrationKind::Plugin, p.id.as_str())),
        )
    {
        settings.insert(
            format!("{}:{id}", kind.key()),
            IntegrationSetting {
                inherited_enabled: enabled(&inherited, kind.table(), id).unwrap_or(true),
                override_enabled: enabled(&local, kind.table(), id),
                plugin_id: None,
            },
        );
    }
    for group in raw_skills["data"].as_array().into_iter().flatten() {
        for raw in group["skills"].as_array().into_iter().flatten() {
            let Some(path) = raw["path"].as_str() else {
                continue;
            };
            let owner = raw["pluginId"].as_str();
            if let Some(skill) = skills.iter_mut().find(|s| s.path == path) {
                if owner
                    .and_then(|id| plugins.iter().find(|p| p.id == id))
                    .is_some_and(|p| !p.enabled)
                {
                    skill.enabled = false;
                }
                settings.insert(
                    format!("skill:{path}"),
                    IntegrationSetting {
                        inherited_enabled: skill.enabled,
                        override_enabled: None,
                        plugin_id: owner.map(str::to_owned),
                    },
                );
            }
        }
    }
    // ConfigToml excludes plugin contributions. Read each installed package's
    // declared inventory, including packages disabled in this project.
    for marketplace in catalog["marketplaces"].as_array().into_iter().flatten() {
        for raw in marketplace["plugins"].as_array().into_iter().flatten() {
            let Some(plugin) = plugins
                .iter()
                .find(|p| Some(p.id.as_str()) == raw["id"].as_str())
            else {
                continue;
            };
            let Some(name) = raw["name"].as_str() else {
                continue;
            };
            let detail = match ctx
                .session
                .send(
                    app,
                    requests::integration_plugin_detail(
                        name,
                        marketplace["path"].as_str(),
                        marketplace["name"].as_str().unwrap_or(""),
                    ),
                )
                .await
            {
                Ok(value) => value,
                Err(_) => {
                    errors.push(format!(
                        "Plugins: Could not load contributions for {}.",
                        plugin.id
                    ));
                    continue;
                }
            };
            for name in detail["plugin"]["mcpServers"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                let id = format!("{}/{name}", plugin.id);
                if mcp_servers.iter().any(|s| s.name == id) {
                    continue;
                }
                let preference = config["config"]["plugins"][&plugin.id]["mcp_servers"][name]
                    ["enabled"]
                    .as_bool()
                    .unwrap_or(true);
                mcp_servers.push(McpServerSummary {
                    name: id.clone(),
                    transport: "plugin".into(),
                    command: None,
                    args: vec![],
                    url: None,
                    env_keys: vec![],
                    bearer_token_env_var: None,
                    enabled: plugin.enabled && preference,
                    scope: "plugin".into(),
                });
                settings.insert(
                    format!("mcp:{id}"),
                    IntegrationSetting {
                        inherited_enabled: enabled(&inherited, "plugins", &plugin.id)
                            .unwrap_or(true)
                            && inherited["plugins"][&plugin.id]["mcp_servers"][name]["enabled"]
                                .as_bool()
                                .unwrap_or(true),
                        override_enabled: local["plugins"][&plugin.id]["mcp_servers"][name]
                            ["enabled"]
                            .as_bool(),
                        plugin_id: Some(plugin.id.clone()),
                    },
                );
            }
            for raw in detail["plugin"]["skills"].as_array().into_iter().flatten() {
                let (Some(path), Some(name)) = (raw["path"].as_str(), raw["name"].as_str()) else {
                    continue;
                };
                if !skills.iter().any(|s| s.path == path) {
                    skills.push(super::SkillSummary {
                        name: name.into(),
                        path: path.into(),
                        scope: "plugin".into(),
                        description: raw["description"].as_str().map(str::to_owned),
                        enabled: plugin.enabled && raw["enabled"].as_bool().unwrap_or(false),
                        display_name: raw["interface"]["displayName"].as_str().map(str::to_owned),
                        short_description: raw["shortDescription"].as_str().map(str::to_owned),
                    });
                }
                let entry = settings
                    .entry(format!("skill:{path}"))
                    .or_insert(IntegrationSetting {
                        inherited_enabled: false,
                        override_enabled: None,
                        plugin_id: None,
                    });
                entry.plugin_id = Some(plugin.id.clone());
                if !plugin.enabled {
                    if let Some(skill) = skills.iter_mut().find(|s| s.path == path) {
                        skill.enabled = false;
                    }
                }
            }
        }
    }
    Ok(IntegrationsList {
        mcp_servers,
        skills,
        plugins,
        plugins_supported,
        settings,
        errors,
    })
}

/// Edit one scalar only; removing an override must not remove transport or auth fields.
#[cfg(test)]
fn edit(
    doc: &mut DocumentMut,
    kind: IntegrationKind,
    id: &str,
    value: Option<bool>,
) -> Result<(), String> {
    edit_path(doc, &[kind.table(), id], value)
}

fn edit_path(doc: &mut DocumentMut, path: &[&str], value: Option<bool>) -> Result<(), String> {
    let mut entry: &mut dyn toml_edit::TableLike = doc.as_table_mut();
    for key in path {
        if key.is_empty() {
            return Err("Missing integration identifier.".into());
        }
        if entry.get(key).is_none() {
            if value.is_none() {
                return Ok(());
            }
            entry.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        entry = entry
            .get_mut(key)
            .and_then(|v| v.as_table_like_mut())
            .ok_or("Integration entry must be a table.")?;
    }
    if let Some(value) = value {
        let mut replacement = toml_edit::Value::from(value);
        if let Some(previous) = entry.get("enabled").and_then(|v| v.as_value()) {
            *replacement.decor_mut() = previous.decor().clone();
        }
        entry.insert("enabled", toml_edit::Item::Value(replacement));
    } else {
        entry.remove("enabled");
    }
    Ok(())
}

fn write_override(directory: &Path, path: &[&str], enabled: Option<bool>) -> Result<(), String> {
    // Serialize this command's read/modify/write operations across windows.
    static EDIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = EDIT_LOCK
        .lock()
        .map_err(|_| "Integration writer is unavailable.")?;
    let mut doc = read_doc(directory)?;
    edit_path(&mut doc, path, enabled)?;
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let file = directory.join("config.toml");
    // Follow an existing config symlink, just as Codex's own editor does.
    let target = if file.exists() {
        file.canonicalize().map_err(|e| e.to_string())?
    } else {
        file
    };
    let mut temporary =
        tempfile::NamedTempFile::new_in(target.parent().ok_or("Config has no parent.")?)
            .map_err(|e| e.to_string())?;
    if let Ok(metadata) = std::fs::metadata(&target) {
        temporary
            .as_file()
            .set_permissions(metadata.permissions())
            .map_err(|e| e.to_string())?;
    }
    temporary
        .write_all(doc.to_string().as_bytes())
        .map_err(|e| e.to_string())?;
    temporary.as_file().sync_all().map_err(|e| e.to_string())?;
    temporary.persist(&target).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn set_integration_enabled(
    kind: IntegrationKind,
    id: String,
    enabled: Option<bool>,
    project_path: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<IntegrationsList, String> {
    let ctx = state.ctx(&window);
    let cwds: Vec<String> = project_path.iter().cloned().collect();
    let before = read(&app, &ctx, cwds.clone(), false).await?;
    if !before
        .settings
        .contains_key(&format!("{}:{id}", kind.key()))
    {
        return Err("Integration is no longer available. Refresh and try again.".into());
    }
    let directory = match project_path {
        Some(ref path) => project_dir(path)?,
        None => ctx.runtime().codex_home,
    };
    let setting = &before.settings[&format!("{}:{id}", kind.key())];
    let path = match (kind, setting.plugin_id.as_deref()) {
        (IntegrationKind::Mcp, Some(owner)) => vec![
            "plugins",
            owner,
            "mcp_servers",
            id.strip_prefix(&format!("{owner}/"))
                .ok_or("Invalid plugin server identifier.")?,
        ],
        _ => vec![kind.table(), &id],
    };
    write_override(&directory, &path, enabled)?;
    let reload = ctx
        .session
        .send(&app, requests::reload_integration_config())
        .await;
    let mut result = read(&app, &ctx, cwds, true)
        .await
        .map_err(|e| format!("Saved, but refresh failed: {e}"))?;
    if reload.is_err() {
        result
            .errors
            .push("Saved. Start a new chat to use the changed integrations.".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn project_files_are_isolated_and_plugin_child_reset_preserves_parent() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        write_override(first.path(), &["plugins", "demo@market"], Some(false)).unwrap();
        write_override(
            first.path(),
            &["plugins", "demo@market", "mcp_servers", "echo"],
            Some(true),
        )
        .unwrap();
        write_override(
            first.path(),
            &["plugins", "demo@market", "mcp_servers", "echo"],
            None,
        )
        .unwrap();
        let doc = read_doc(first.path()).unwrap();
        assert_eq!(
            doc["plugins"]["demo@market"]["enabled"].as_bool(),
            Some(false)
        );
        assert!(doc["plugins"]["demo@market"]["mcp_servers"]["echo"]
            .get("enabled")
            .is_none());
        assert!(read_doc(second.path()).unwrap().is_empty());
    }

    #[test]
    fn settings_keep_duplicate_names_at_distinct_paths() {
        let skills = parse_skill_entries(
            &json!({"data":[{"skills":[
                {"name":"demo","path":"/home/SKILL.md"}, {"name":"demo","path":"/project/SKILL.md"}, {"name":"demo","path":"/home/SKILL.md"}
            ]}]}),
            true,
        );
        assert_eq!(skills.len(), 2);
    }

    #[test]
    fn higher_inherited_layer_wins() {
        let config = json!({"layers":[
            {"name":{"type":"sessionFlags"},"config":{"plugins":{"demo":{"enabled":false}}}},
            {"name":{"type":"user","file":"/home/config.toml"},"config":{"plugins":{"demo":{"enabled":true}}}}
        ]});
        assert_eq!(
            enabled(
                &inherited_config(&config, Path::new("/project/.codex")),
                "plugins",
                "demo"
            ),
            Some(false)
        );
    }
    #[test]
    fn override_and_reset_preserve_transport_credentials_and_comments() {
        let mut doc: DocumentMut = "# keep\n[mcp_servers.demo]\ncommand = 'server'\n[mcp_servers.demo.env]\nTOKEN = 'secret'\n".parse().unwrap();
        edit(&mut doc, IntegrationKind::Mcp, "demo", Some(false)).unwrap();
        assert_eq!(doc["mcp_servers"]["demo"]["enabled"].as_bool(), Some(false));
        edit(&mut doc, IntegrationKind::Mcp, "demo", None).unwrap();
        assert!(doc["mcp_servers"]["demo"].get("enabled").is_none());
        assert_eq!(
            doc["mcp_servers"]["demo"]["env"]["TOKEN"].as_str(),
            Some("secret")
        );
        assert!(doc.to_string().contains("# keep"));
    }
    #[test]
    fn installed_plugins_keep_marketplace_identity() {
        let raw = json!({"marketplaces":[{"name":"market", "plugins":[
            {"id":"demo@a","name":"demo","installed":true,"enabled":false},
            {"id":"demo@b","name":"demo","installed":true,"enabled":true},
            {"id":"other@a","name":"other","installed":false,"enabled":false}
        ]}]});
        let plugins = parse_plugins(&raw);
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].id, "demo@a");
        assert!(!plugins[0].enabled);
    }
    #[test]
    fn inheritance_ignores_disabled_and_selected_layers() {
        let raw = json!({"layers":[
            {"name":{"type":"user","file":"/home/config.toml"},"config":{"plugins":{"demo":{"enabled":false}}}},
            {"name":{"type":"project","dotCodexFolder":"/repo/.codex"},"config":{"plugins":{"demo":{"enabled":true}}}},
            {"name":{"type":"project","dotCodexFolder":"/other/.codex"},"disabledReason":"untrusted","config":{"plugins":{"demo":{"enabled":true}}}}
        ]});
        assert_eq!(
            enabled(
                &inherited_config(&raw, Path::new("/repo/.codex")),
                "plugins",
                "demo"
            ),
            Some(false)
        );
    }
}
