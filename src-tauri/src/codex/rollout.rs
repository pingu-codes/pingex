//! What Codex wrote into a thread's context, read back from its rollout file.
//!
//! Codex keeps every thread as a JSONL rollout under `<codex_home>/sessions`.
//! It has no API for what the system prompt holds, but the file has the text
//! itself: the base instructions in the session meta line, and every
//! instruction block it placed in a developer or user message — AGENTS.md,
//! skills, permissions, environment, memory, apps, plugins, compaction
//! summaries. This module sizes those blocks into neutral Prompt parts
//! (`usage::prompt_parts`) so the usage view can split the "System prompt"
//! slice. Codex's vocabulary (record types, markers, content kinds) stays here.
//!
//! Tool definitions are never written to disk; they are what remains once the
//! named parts are taken out of the measured system prompt.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde_json::Value;

use crate::usage::estimate::tokens_for_bytes;
use crate::usage::prompt_parts::{PromptPart, PromptPartKind};

/// The rollout file of `thread_id` under `sessions` (or `archived_sessions`)
/// in a local Codex home, if it exists. Rollouts are named
/// `rollout-<timestamp>-<thread_id>.jsonl` and filed by date, so this walks
/// the year/month/day folders looking for that suffix.
pub(crate) fn locate(local_home: &Path, thread_id: &str) -> Option<PathBuf> {
    let suffix = format!("-{thread_id}.jsonl");
    ["sessions", "archived_sessions"]
        .iter()
        .find_map(|folder| find_with_suffix(&local_home.join(folder), &suffix, 4))
}

fn find_with_suffix(dir: &Path, suffix: &str, depth: usize) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut folders = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            folders.push(path);
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            return Some(path);
        }
    }
    if depth == 0 {
        return None;
    }
    folders.sort();
    folders
        .iter()
        .rev()
        .find_map(|folder| find_with_suffix(folder, suffix, depth - 1))
}

/// Parsed parts per rollout file, kept until the file grows or changes.
/// Parsing streams the whole file; the usage panel asks on every token
/// update, so the answer is cached by `(len, mtime)`.
/// A file's size and mtime when it was parsed, and what came out.
type CachedParts = (u64, Option<SystemTime>, Vec<PromptPart>);

#[derive(Default)]
pub(crate) struct RolloutCache {
    entries: Mutex<HashMap<PathBuf, CachedParts>>,
}

impl RolloutCache {
    pub(crate) fn prompt_parts(&self, path: &Path) -> Result<Vec<PromptPart>, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let stamp = (meta.len(), meta.modified().ok());
        if let Ok(entries) = self.entries.lock() {
            if let Some((len, modified, parts)) = entries.get(path) {
                if (*len, *modified) == stamp {
                    return Ok(parts.clone());
                }
            }
        }
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let parts = prompt_parts_from_rollout(BufReader::new(file), false);
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(path.to_path_buf(), (stamp.0, stamp.1, parts.clone()));
        }
        Ok(parts)
    }

    /// The same parts, with each one's full text — parsed fresh every call
    /// since this only runs on an explicit user request, not on every token
    /// update, so it is not worth caching text nobody has asked to see yet.
    pub(crate) fn prompt_parts_with_text(&self, path: &Path) -> Result<Vec<PromptPart>, String> {
        let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(prompt_parts_from_rollout(BufReader::new(file), true))
    }
}

/// Codex's own markers for the blocks it puts in front of the model
/// (`codex-rs/protocol/src/protocol.rs`) and the prefixes of the few that
/// have none. Matched against the start of a content entry's text when the
/// entry carries no content kind (rollouts before 0.150).
const MARKERS: &[(&str, PromptPartKind)] = &[
    ("# AGENTS.md instructions", PromptPartKind::AgentsMd),
    ("<user_instructions>", PromptPartKind::AgentsMd),
    ("<skills_instructions>", PromptPartKind::Skills),
    ("<permissions instructions>", PromptPartKind::Permissions),
    ("<environment_context>", PromptPartKind::Environment),
    ("<environments_instructions>", PromptPartKind::Environment),
    ("<context_window>", PromptPartKind::Environment),
    ("<context_window_guidance>", PromptPartKind::Environment),
    ("## Memory", PromptPartKind::Memory),
    ("<collaboration_mode>", PromptPartKind::Memory),
    ("<multi_agent_mode>", PromptPartKind::Memory),
    ("<app-context>", PromptPartKind::Apps),
    ("<apps_instructions>", PromptPartKind::Apps),
    ("<plugins_instructions>", PromptPartKind::Plugins),
    ("<tools>", PromptPartKind::Other),
    ("<realtime_conversation>", PromptPartKind::Other),
    (
        "Another language model started to solve this problem",
        PromptPartKind::Compaction,
    ),
];

/// Content kinds (0.150+) by their namespace or full name.
fn kind_for_content_kind(kind: &str) -> Option<Option<PromptPartKind>> {
    let namespace = kind.split('.').next().unwrap_or(kind);
    let mapped = match (namespace, kind) {
        (_, "model.base_instructions") => PromptPartKind::BaseInstructions,
        ("agents_md", _) => PromptPartKind::AgentsMd,
        ("apps", _) => PromptPartKind::Apps,
        ("plugins", _) => PromptPartKind::Plugins,
        ("environments" | "token_budget" | "rollout_budget", _) => PromptPartKind::Environment,
        ("permissions", _) => PromptPartKind::Permissions,
        (_, "compaction.summary") => PromptPartKind::Compaction,
        (_, "tools.deferred_namespaces") | ("realtime_conversation", _) => PromptPartKind::Other,
        (
            "generic" | "managed_config" | "collaboration_mode" | "personality" | "persistent_mode"
            | "model_switch" | "hooks" | "guardian" | "extension",
            _,
        )
        | (
            _,
            "multi_agent.mode_instructions"
            | "multi_agent.role_instructions"
            | "multi_agent.usage_hint",
        ) => PromptPartKind::Memory,
        // The user's own words, media, inter-agent traffic, one-off notices:
        // not the prompt, or already another category.
        (
            "user" | "shell" | "multi_agent" | "images" | "audio" | "current_time"
            | "user_verification",
            _,
        ) => return Some(None),
        _ => return None,
    };
    Some(Some(mapped))
}

fn label_for(kind: PromptPartKind) -> &'static str {
    match kind {
        PromptPartKind::BaseInstructions => "Base instructions",
        PromptPartKind::AgentsMd => "AGENTS.md",
        PromptPartKind::Skills => "Skills instructions",
        PromptPartKind::Permissions => "Permissions",
        PromptPartKind::Environment => "Environment context",
        PromptPartKind::Memory => "Memory and developer instructions",
        PromptPartKind::Apps => "Apps",
        PromptPartKind::Plugins => "Plugins",
        PromptPartKind::Compaction => "Compaction summary",
        PromptPartKind::Tools => "Tool definitions",
        PromptPartKind::Other => "Other instructions",
    }
}

/// The directory an AGENTS.md block is for, from the header Codex writes:
/// `# AGENTS.md instructions for <dir>`.
fn agents_md_directory(text: &str) -> Option<String> {
    let first = text.lines().next()?;
    let rest = first.strip_prefix("# AGENTS.md instructions")?;
    rest.strip_prefix(" for ")
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .map(str::to_string)
}

/// RFC 7386 merge patch: objects merge recursively, `null` deletes, anything
/// else replaces. Codex writes incremental `world_state` records this way.
fn merge_patch(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target), Value::Object(patch)) => {
            for (key, value) in patch {
                if value.is_null() {
                    target.remove(key);
                } else {
                    merge_patch(target.entry(key.clone()).or_insert(Value::Null), value);
                }
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

/// One block Codex placed in the context: its kind, what tells it apart from
/// other blocks of that kind (the marker, the content kind, or an AGENTS.md
/// directory), and what the part's `detail` should say.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Block {
    kind: PromptPartKind,
    key: String,
    detail: Option<String>,
}

#[derive(Default)]
struct Collector {
    /// Bytes per block; the latest occurrence of a block wins, since Codex
    /// re-sends one when it changes and after a compaction. Blocks of one
    /// kind are summed when the parts are built.
    bytes: BTreeMap<Block, u64>,
    /// A block's text, kept only when `with_text` is set — the sized-only
    /// path (polled on every token update) never pays to hold it.
    texts: BTreeMap<Block, String>,
    with_text: bool,
    world_state: Value,
}

impl Collector {
    fn new(with_text: bool) -> Self {
        Self {
            with_text,
            ..Self::default()
        }
    }

    fn record(&mut self, kind: PromptPartKind, key: &str, detail: Option<String>, text: &str) {
        let block = Block {
            kind,
            key: key.to_string(),
            detail,
        };
        self.bytes.insert(block.clone(), text.len() as u64);
        if self.with_text {
            self.texts.insert(block, text.to_string());
        }
    }

    fn agents_directory(&self) -> Option<String> {
        self.world_state
            .pointer("/agents_md/directory")
            .and_then(Value::as_str)
            .map(str::to_string)
    }

    fn content_entry(&mut self, role: &str, kind: Option<&str>, text: &str) {
        let by_kind = kind.and_then(kind_for_content_kind);
        let part = match by_kind {
            Some(Some(kind)) => Some(kind),
            Some(None) => None,
            None => {
                let by_marker = MARKERS
                    .iter()
                    .find(|(marker, _)| text.starts_with(marker))
                    .map(|(_, kind)| *kind);
                // A developer message is the harness talking; anything in
                // one that nothing names is developer instructions.
                by_marker.or((role == "developer").then_some(PromptPartKind::Memory))
            }
        };
        let Some(part) = part else { return };
        let marker = MARKERS
            .iter()
            .find(|(marker, _)| text.starts_with(marker))
            .map(|(marker, _)| *marker);
        let detail = match part {
            PromptPartKind::AgentsMd => {
                agents_md_directory(text).or_else(|| self.agents_directory())
            }
            PromptPartKind::Other => Some(marker.map_or_else(
                || "other".to_string(),
                |m| m.trim_matches(['<', '>']).replace('_', " "),
            )),
            _ => None,
        };
        // What tells this block from others of its kind: the directory, the
        // content kind, the marker, or — for untagged, unmarked developer
        // text — its opening words.
        let key = detail
            .clone()
            .or_else(|| kind.map(str::to_string))
            .or_else(|| marker.map(str::to_string))
            .unwrap_or_else(|| text.chars().take(40).collect());
        self.record(part, &key, detail, text);
    }

    fn line(&mut self, record: &Value) {
        let Some(kind) = record.get("type").and_then(Value::as_str) else {
            return;
        };
        let payload = record.get("payload").unwrap_or(&Value::Null);
        match kind {
            "session_meta" => {
                if let Some(text) = payload
                    .pointer("/base_instructions/text")
                    .and_then(Value::as_str)
                {
                    self.record(PromptPartKind::BaseInstructions, "base", None, text);
                }
            }
            "world_state" => {
                let state = payload.get("state").cloned().unwrap_or(Value::Null);
                if payload.get("full").and_then(Value::as_bool).unwrap_or(true) {
                    self.world_state = state;
                } else {
                    merge_patch(&mut self.world_state, &state);
                }
            }
            "compacted" => {
                if let Some(text) = payload.get("message").and_then(Value::as_str) {
                    self.record(PromptPartKind::Compaction, "compaction", None, text);
                }
            }
            "response_item" => {
                if payload.get("type").and_then(Value::as_str) != Some("message") {
                    return;
                }
                let role = payload
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if role != "developer" && role != "user" {
                    return;
                }
                let kinds = payload
                    .pointer("/internal_chat_message_metadata_passthrough/content_item_kinds")
                    .and_then(Value::as_array);
                let Some(content) = payload.get("content").and_then(Value::as_array) else {
                    return;
                };
                for (index, entry) in content.iter().enumerate() {
                    let Some(text) = entry.get("text").and_then(Value::as_str) else {
                        continue;
                    };
                    let kind = kinds
                        .and_then(|kinds| kinds.get(index))
                        .and_then(Value::as_str);
                    self.content_entry(role, kind, text);
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> Vec<PromptPart> {
        let mut by_part: BTreeMap<(PromptPartKind, Option<String>), u64> = BTreeMap::new();
        for (block, bytes) in &self.bytes {
            // A compaction summary replaces the compacted record's message
            // rather than adding to it; everything else of a kind adds up.
            let entry = by_part.entry((block.kind, block.detail.clone())).or_insert(0);
            if block.kind == PromptPartKind::Compaction {
                *entry = *bytes;
            } else {
                *entry += bytes;
            }
        }
        let mut by_text: BTreeMap<(PromptPartKind, Option<String>), String> = BTreeMap::new();
        if self.with_text {
            for (block, text) in &self.texts {
                let entry = by_text
                    .entry((block.kind, block.detail.clone()))
                    .or_default();
                if block.kind == PromptPartKind::Compaction {
                    *entry = text.clone();
                } else if entry.is_empty() {
                    *entry = text.clone();
                } else {
                    entry.push_str("\n\n");
                    entry.push_str(text);
                }
            }
        }
        by_part
            .into_iter()
            .map(|((kind, detail), bytes)| PromptPart {
                kind,
                label: label_for(kind).to_string(),
                tokens: tokens_for_bytes(bytes),
                source: crate::usage::prompt_parts::PartSource::Estimated,
                text: by_text.get(&(kind, detail.clone())).cloned(),
                detail,
            })
            .collect()
    }
}

/// Records worth parsing; everything else (token counts, turn context, tool
/// calls, reasoning) is skipped before the JSON is read.
const INTERESTING: &[&str] = &["session_meta", "world_state", "response_item", "compacted"];

/// The Prompt parts a rollout describes, sized at Codex's own four bytes to
/// a token. A line that does not parse — the one Codex is still writing —
/// is skipped.
pub(crate) fn prompt_parts_from_rollout(reader: impl BufRead, with_text: bool) -> Vec<PromptPart> {
    let mut collector = Collector::new(with_text);
    for line in reader.lines().map_while(Result::ok) {
        if !INTERESTING.iter().any(|kind| line.contains(kind)) {
            continue;
        }
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        collector.line(&record);
    }
    collector.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const FIXTURE: &str =
        include_str!("../../../tests/fixtures/protocol/codex/rollout-prompt-parts.jsonl");

    fn parts() -> Vec<PromptPart> {
        prompt_parts_from_rollout(Cursor::new(FIXTURE), false)
    }

    fn parts_with_text() -> Vec<PromptPart> {
        prompt_parts_from_rollout(Cursor::new(FIXTURE), true)
    }

    #[test]
    fn with_text_keeps_each_parts_text() {
        let base = parts_with_text()
            .into_iter()
            .find(|p| p.kind == PromptPartKind::BaseInstructions)
            .expect("base instructions part");
        assert_eq!(
            base.text.as_deref(),
            Some("You are Codex. Be concise and be careful")
        );
    }

    fn tokens(kind: PromptPartKind) -> Vec<(Option<String>, u64)> {
        parts()
            .into_iter()
            .filter(|p| p.kind == kind)
            .map(|p| (p.detail, p.tokens))
            .collect()
    }

    #[test]
    fn the_base_instructions_come_from_the_session_meta() {
        // "You are Codex." plus a second sentence: 40 bytes → 10 tokens.
        assert_eq!(tokens(PromptPartKind::BaseInstructions), vec![(None, 10)]);
    }

    #[test]
    fn every_marker_is_classified_and_labelled() {
        let kinds: Vec<PromptPartKind> = parts().into_iter().map(|p| p.kind).collect();
        for expected in [
            PromptPartKind::AgentsMd,
            PromptPartKind::Skills,
            PromptPartKind::Permissions,
            PromptPartKind::Environment,
            PromptPartKind::Memory,
            PromptPartKind::Apps,
            PromptPartKind::Plugins,
            PromptPartKind::Compaction,
        ] {
            assert!(
                kinds.contains(&expected),
                "missing {expected:?} in {kinds:?}"
            );
        }
        assert!(parts().iter().all(|p| p.tokens > 0));
        assert!(parts().iter().any(|p| p.label == "Skills instructions"));
    }

    #[test]
    fn content_kinds_win_over_markers_and_user_text_is_not_a_part() {
        // The tagged message has an `agents_md.instructions` entry with no
        // header, and a `user.text` entry that starts with "## Memory".
        let agents = tokens(PromptPartKind::AgentsMd);
        assert_eq!(agents.len(), 2, "one per directory: {agents:?}");
        assert!(agents
            .iter()
            .any(|(dir, _)| dir.as_deref() == Some("/repo/sub")));
        let memory = tokens(PromptPartKind::Memory);
        assert_eq!(memory.len(), 1, "user text is never memory");
        // "## Memory…" (27 bytes) and the collaboration mode block (48) add up.
        assert_eq!(memory[0].1, tokens_for_bytes(27 + 48));
    }

    #[test]
    fn the_latest_occurrence_of_a_block_wins() {
        // <environment_context> appears twice; the second is longer.
        let env = tokens(PromptPartKind::Environment);
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].1, tokens_for_bytes(67));
        // /repo AGENTS.md is re-sent after the compaction with a longer body.
        let repo = tokens(PromptPartKind::AgentsMd)
            .into_iter()
            .find(|(dir, _)| dir.as_deref() == Some("/repo"))
            .expect("/repo");
        assert_eq!(repo.1, tokens_for_bytes(88));
    }

    #[test]
    fn a_compaction_is_counted_once_from_its_summary() {
        let compaction = tokens(PromptPartKind::Compaction);
        assert_eq!(compaction.len(), 1);
        // The summary item (kind compaction.summary) replaces the compacted
        // record's message.
        assert_eq!(compaction[0].1, tokens_for_bytes(76));
    }

    #[test]
    fn the_world_state_merge_patch_names_the_agents_directory() {
        let mut state =
            serde_json::json!({"agents_md": {"directory": "/a", "hash": "x"}, "keep": 1});
        merge_patch(
            &mut state,
            &serde_json::json!({"agents_md": {"directory": "/b"}, "keep": null}),
        );
        assert_eq!(
            state,
            serde_json::json!({"agents_md": {"directory": "/b", "hash": "x"}})
        );
    }

    #[test]
    fn a_truncated_last_line_is_skipped() {
        let mut truncated = FIXTURE.to_string();
        truncated.push_str(r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"<skills_instruc"#);
        assert_eq!(
            prompt_parts_from_rollout(Cursor::new(truncated), false),
            parts()
        );
    }

    /// Sizes a real rollout: `CODEX_ROLLOUT=~/.codex/sessions/.../rollout-….jsonl
    /// cargo test --lib rollout::tests::a_real_rollout -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn a_real_rollout_sizes_its_prompt_parts() {
        let path = std::env::var("CODEX_ROLLOUT").expect("CODEX_ROLLOUT");
        let file = File::open(&path).expect("open rollout");
        let parts = prompt_parts_from_rollout(BufReader::new(file), false);
        for part in &parts {
            eprintln!(
                "{:>7} {:?} {} {:?}",
                part.tokens, part.kind, part.label, part.detail
            );
        }
        assert!(!parts.is_empty());
    }

    #[test]
    fn locate_finds_the_rollout_by_thread_id_suffix() {
        let dir = tempfile::tempdir().expect("tempdir");
        let day = dir.path().join("sessions/2026/09/10");
        std::fs::create_dir_all(&day).expect("mkdir");
        let file = day.join("rollout-2026-09-10T10-00-00-abc123.jsonl");
        std::fs::write(&file, "").expect("write");
        assert_eq!(locate(dir.path(), "abc123"), Some(file));
        assert_eq!(locate(dir.path(), "nope"), None);
    }
}
