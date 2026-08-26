use serde::Serialize;
use std::fs;
use std::path::Path;

/// A single MCP server declared in `config.toml`. We only surface the name and
/// launch command — never secrets like `env` blocks or headers.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpServerInfo {
    pub(crate) name: String,
    pub(crate) command: Option<String>,
}

/// A skill as Codex resolves it. Supplied by the caller rather than scraped off
/// disk — Codex looks in several roots and honours enable/disable state, so a
/// `read_dir` of `CODEX_HOME/skills` disagreed with the Integrations tab.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SkillInfo {
    pub(crate) name: String,
}

/// Read-only snapshot of the active Codex home's default configuration, shown
/// on the homepage dashboard. Everything is best-effort: a missing or
/// unparseable `config.toml`, or an unreachable Codex, simply yields empty
/// fields rather than an error.
#[derive(Debug, Serialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HomeOverview {
    pub(crate) codex_home: String,
    pub(crate) codex_binary: String,
    pub(crate) config_exists: bool,
    pub(crate) model: Option<String>,
    pub(crate) reasoning_effort: Option<String>,
    pub(crate) approval_policy: Option<String>,
    pub(crate) sandbox_mode: Option<String>,
    pub(crate) mcp_servers: Vec<McpServerInfo>,
    pub(crate) skills: Vec<SkillInfo>,
}

fn string_field(table: &toml::Table, key: &str) -> Option<String> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .map(str::to_string)
}

/// Parse the `[mcp_servers.*]` tables into name/command pairs, sorted by name.
fn mcp_servers_from(config: &toml::Table) -> Vec<McpServerInfo> {
    let Some(servers) = config.get("mcp_servers").and_then(toml::Value::as_table) else {
        return Vec::new();
    };
    let mut servers: Vec<McpServerInfo> = servers
        .iter()
        .map(|(name, value)| McpServerInfo {
            name: name.clone(),
            command: value
                .as_table()
                .and_then(|table| table.get("command"))
                .and_then(toml::Value::as_str)
                .map(str::to_string),
        })
        .collect();
    servers.sort_by(|a, b| a.name.cmp(&b.name));
    servers
}

pub(crate) fn read_home_overview(
    codex_home: &Path,
    codex_binary: &str,
    skills: Vec<SkillInfo>,
) -> HomeOverview {
    let config = fs::read_to_string(codex_home.join("config.toml"))
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok());
    let config_exists = config.is_some();
    let (model, reasoning_effort, approval_policy, sandbox_mode, mcp_servers) = match &config {
        Some(config) => (
            string_field(config, "model"),
            string_field(config, "model_reasoning_effort"),
            string_field(config, "approval_policy"),
            string_field(config, "sandbox_mode"),
            mcp_servers_from(config),
        ),
        None => (None, None, None, None, Vec::new()),
    };
    HomeOverview {
        codex_home: codex_home.display().to_string(),
        codex_binary: codex_binary.to_string(),
        config_exists,
        model,
        reasoning_effort,
        approval_policy,
        sandbox_mode,
        mcp_servers,
        skills,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_config_and_skills_are_tolerated() {
        let directory = tempfile::tempdir().unwrap();
        let overview = read_home_overview(directory.path(), "codex", Vec::new());
        assert!(!overview.config_exists);
        assert_eq!(overview.model, None);
        assert!(overview.mcp_servers.is_empty());
        assert!(overview.skills.is_empty());
        assert_eq!(overview.codex_binary, "codex");
    }

    #[test]
    fn parses_config_defaults_and_mcp_servers() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("config.toml"),
            r#"
model = "gpt-5.6-luna"
model_reasoning_effort = "xhigh"
approval_policy = "on-request"
sandbox_mode = "workspace-write"

[mcp_servers.zeta]
command = "/usr/bin/zeta"

[mcp_servers.alpha]
command = "run-alpha"

[mcp_servers.alpha.env]
SECRET = "should-not-be-read"

[mcp_servers.no_command]
url = "https://example.com"
"#,
        )
        .unwrap();

        let overview = read_home_overview(directory.path(), "/opt/homebrew/bin/codex", Vec::new());
        assert!(overview.config_exists);
        assert_eq!(overview.model.as_deref(), Some("gpt-5.6-luna"));
        assert_eq!(overview.reasoning_effort.as_deref(), Some("xhigh"));
        assert_eq!(overview.approval_policy.as_deref(), Some("on-request"));
        assert_eq!(overview.sandbox_mode.as_deref(), Some("workspace-write"));

        // Sorted by name; secrets in `env` are never surfaced.
        let names: Vec<_> = overview
            .mcp_servers
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, vec!["alpha", "no_command", "zeta"]);
        assert_eq!(
            overview.mcp_servers[0].command.as_deref(),
            Some("run-alpha")
        );
        assert_eq!(overview.mcp_servers[1].command, None);
    }

    #[test]
    fn corrupt_config_yields_empty_overview() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join("config.toml"), "this is = = not toml").unwrap();
        let overview = read_home_overview(directory.path(), "codex", Vec::new());
        assert!(!overview.config_exists);
        assert!(overview.mcp_servers.is_empty());
    }
}
