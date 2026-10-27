//! maki (github.com/tontinton/maki) as an ACP agent: `maki acp`.
//!
//! Wire habits checked against 0.6.5 (`docs/SUPPORTED_VERSIONS.md`): a tool
//! call opens as `pending` with only its tool name for a title, and its input
//! arrives with `in_progress`; output comes back fenced in ``` with the plain
//! text in `rawOutput`; writes inside the project carry no diff, so one is
//! built from the input; `usage_update.cost` is cumulative per process.

use serde_json::{json, Value};

use super::spec::{model_entry, AcpMode, AcpSpec, SpawnSettings, DEFAULT_MODEL};
use crate::harness::{HarnessKind, ToolCallContent};
use crate::util::host::Host;
use crate::util::json::str_at;
use crate::util::stdio::strip_ansi;

pub(crate) struct Maki;

const MODES: [AcpMode; 3] = [
    AcpMode {
        id: "build",
        label: "Build",
        description: "Asks before commands, web fetches and writes outside the project",
        session_mode: "build",
        skips_prompts: false,
        plan: false,
    },
    AcpMode {
        id: "yolo",
        label: "Build without prompts",
        description: "Runs everything without asking; maki's deny rules still apply",
        session_mode: "build",
        skips_prompts: true,
        plan: false,
    },
    AcpMode {
        id: "plan",
        label: "Plan",
        description: "Reads and plans without changing anything",
        session_mode: "plan",
        skips_prompts: false,
        plan: true,
    },
];

impl AcpSpec for Maki {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Maki
    }

    fn default_binary(&self) -> &'static str {
        "maki"
    }

    fn binary_env(&self) -> &'static str {
        "PINGEX_MAKI_CLI_PATH"
    }

    fn version_floor(&self) -> &'static str {
        "0.6.5"
    }

    fn config_dir(&self, host: &Host) -> String {
        let home = host.home_dir().unwrap_or_else(|| "~".to_string());
        host.join_str(&home, ".config/maki")
    }

    fn args(&self, settings: &SpawnSettings) -> Vec<String> {
        args(settings)
    }

    fn modes(&self) -> &'static [AcpMode] {
        &MODES
    }

    fn logged_in(&self, host: &Host, program: &str) -> Option<bool> {
        let output = host
            .command(program, &["auth", "status"], None, &[], &[])
            .output()
            .ok()?;
        logged_in(&String::from_utf8_lossy(&output.stdout))
    }

    fn models(&self, host: &Host, program: &str) -> Result<Value, String> {
        let output = host
            .command(program, &["models"], None, &[], &[])
            .output()
            .map_err(|error| format!("Could not list maki's models: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "maki models failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        Ok(models(&String::from_utf8_lossy(&output.stdout)))
    }

    fn notice_prefix(&self) -> Option<&'static str> {
        Some("notice-")
    }

    fn tool_diff(&self, name: &str, raw_input: &Value) -> Vec<ToolCallContent> {
        tool_diff(name, raw_input)
    }

    fn delete_session_args(&self, session_id: &str) -> Option<Vec<String>> {
        Some(vec!["session".into(), "delete".into(), session_id.into()])
    }
}

/// `maki acp [-m model] [--yolo]`. Public through `e2e` so the live suite
/// launches exactly what the app does.
pub fn args(settings: &SpawnSettings) -> Vec<String> {
    let mut args = vec!["acp".to_string()];
    if let Some(model) = settings
        .model
        .as_deref()
        .filter(|model| !model.is_empty() && *model != DEFAULT_MODEL)
    {
        args.extend(["-m".into(), model.into()]);
    }
    if settings.skips_prompts {
        args.push("--yolo".into());
    }
    args
}

/// `maki auth status` prints one line per provider, ✓ for those with a
/// credential. Any ✓ is enough to run something.
fn logged_in(stdout: &str) -> Option<bool> {
    let text = strip_ansi(stdout);
    if text.trim().is_empty() {
        return None;
    }
    Some(text.lines().any(|line| line.trim_start().starts_with('✓')))
}

/// `maki models` prints `provider/model` per line, for the providers it can
/// reach; anything else on stdout is skipped.
fn models(stdout: &str) -> Value {
    // Its last-used model, or `provider.default_model` in `init.lua`.
    let mut data = vec![model_entry(
        DEFAULT_MODEL,
        "Default",
        "Whatever maki last used, or its configured default",
        true,
    )];
    for line in stdout.lines().map(str::trim) {
        let Some((provider, model)) = line.split_once('/') else {
            continue;
        };
        if provider.is_empty() || model.is_empty() || line.contains(char::is_whitespace) {
            continue;
        }
        data.push(model_entry(line, model, provider, false));
    }
    json!({"data": data})
}

fn tool_diff(name: &str, input: &Value) -> Vec<ToolCallContent> {
    let Some(path) = str_at(input, "path").or_else(|| str_at(input, "file_path")) else {
        return Vec::new();
    };
    let diff = |old: Option<&str>, new: &str| ToolCallContent::Diff {
        path: path.to_string(),
        old_text: old.map(str::to_string),
        new_text: new.to_string(),
    };
    match name {
        // An append is not the whole file, so it would read as a rewrite.
        "write" if input.get("append").and_then(Value::as_bool) != Some(true) => {
            str_at(input, "content")
                .map(|content| vec![diff(None, content)])
                .unwrap_or_default()
        }
        "edit" => match (str_at(input, "old_string"), str_at(input, "new_string")) {
            (Some(old), Some(new)) => vec![diff(Some(old), new)],
            _ => Vec::new(),
        },
        "multiedit" => input
            .get("edits")
            .and_then(Value::as_array)
            .map(|edits| {
                edits
                    .iter()
                    .filter_map(|edit| {
                        Some(diff(
                            Some(str_at(edit, "old_string")?),
                            str_at(edit, "new_string")?,
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_model_and_prompting_modes_add_no_flags() {
        assert_eq!(args(&SpawnSettings::default()), vec!["acp"]);
        assert_eq!(
            args(&SpawnSettings {
                model: Some(DEFAULT_MODEL.into()),
                skips_prompts: false
            }),
            vec!["acp"]
        );
        assert_eq!(
            args(&SpawnSettings {
                model: Some("openai/gpt-6-luna".into()),
                skips_prompts: true
            }),
            vec!["acp", "-m", "openai/gpt-6-luna", "--yolo"]
        );
    }

    #[test]
    fn any_tick_in_auth_status_is_a_login() {
        let status = "\n  \u{1b}[31m✗\u{1b}[0m anthropic      Anthropic (run: maki auth login anthropic)\n  \u{1b}[32m✓\u{1b}[0m openai         OpenAI (oauth)\n";
        assert_eq!(logged_in(status), Some(true));
        assert_eq!(
            logged_in("  \u{1b}[31m✗\u{1b}[0m xai  xAI (run: maki auth login xai)\n"),
            Some(false)
        );
        assert_eq!(logged_in(""), None);
    }

    #[test]
    fn models_are_provider_slash_id_lines_after_a_default() {
        let list = models(
            "warning: Ollama: http: failed to connect\nopenai/gpt-6-luna\nollama-cloud/gpt-oss:120b\n",
        );
        let ids: Vec<&str> = list["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|model| model["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec![
                DEFAULT_MODEL,
                "openai/gpt-6-luna",
                "ollama-cloud/gpt-oss:120b"
            ]
        );
        assert_eq!(list["data"][1]["displayName"], "gpt-6-luna");
        assert_eq!(list["data"][0]["isDefault"], true);
    }

    #[test]
    fn writes_and_edits_become_diffs_and_appends_do_not() {
        let write = tool_diff("write", &json!({"path": "/p/note.txt", "content": "hi\n"}));
        assert!(matches!(
            &write[..],
            [ToolCallContent::Diff { path, old_text: None, new_text }] if path == "/p/note.txt" && new_text == "hi\n"
        ));
        assert!(tool_diff(
            "write",
            &json!({"path": "/p/log", "content": "x", "append": true})
        )
        .is_empty());
        let edit = tool_diff(
            "multiedit",
            &json!({"path": "/p/a.rs", "edits": [
                {"old_string": "a", "new_string": "b"},
                {"old_string": "c", "new_string": "d"},
            ]}),
        );
        assert_eq!(edit.len(), 2);
        assert!(tool_diff("bash", &json!({"command": "ls"})).is_empty());
    }

    #[test]
    fn version_is_the_last_word() {
        assert_eq!(Maki.version_of("maki 0.6.5").as_deref(), Some("0.6.5"));
    }

    #[test]
    fn a_session_mode_maps_back_through_how_the_process_runs() {
        assert_eq!(Maki.app_mode("build", false).unwrap().id, "build");
        assert_eq!(Maki.app_mode("build", true).unwrap().id, "yolo");
        assert_eq!(Maki.app_mode("plan", true).unwrap().id, "plan");
    }
}
