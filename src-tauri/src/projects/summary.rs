//! Turning raw app-server thread values into the summaries the sidebar and the
//! search index store.
//!
//! Both projections read the same loosely-typed JSON and must agree on the
//! title, which is why they live together.

use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::types::ThreadSummary;
use crate::storage::{StoredThreadSearch, StoredThreadSummary};
use crate::util::json::{i64_at, str_at};

/// How much of a thread's first line is kept as its title.
const TITLE_CHARS: usize = 80;
/// How much body text the search index keeps for matching and previews.
const PREVIEW_CHARS: usize = 400;

/// The thread's title text: its name, or its preview when unnamed. Blank values
/// fall through so an empty name does not shadow a usable preview.
fn title_text(thread: &Value) -> Option<&str> {
    str_at(thread, "name")
        .filter(|value| !value.trim().is_empty())
        .or_else(|| str_at(thread, "preview").filter(|value| !value.trim().is_empty()))
}

/// The first line of `text`, de-marked-up and truncated to a one-line title.
fn title_from(text: &str) -> String {
    strip_mention_markup(text.lines().next().unwrap_or(text))
        .chars()
        .take(TITLE_CHARS)
        .collect()
}

/// Codex stores `@` file mentions as markdown links (`[index.ts](src/index.ts)`),
/// which reads as noise in a one-line thread title. Collapse each one back to
/// `@name`; genuine prose links (`[docs](https://…)`) keep their label.
pub(crate) fn strip_mention_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(']') else {
            break;
        };
        let label = &after_open[..close];
        let tail = &after_open[close + 1..];
        let target = tail
            .strip_prefix('(')
            .and_then(|inner| inner.find(')').map(|end| &inner[..end]));
        match target.filter(|target| is_mention_target(label, target)) {
            Some(target) => {
                out.push_str(&rest[..open]);
                out.push('@');
                out.push_str(label);
                rest = &tail[target.len() + 2..];
            }
            None => {
                out.push_str(&rest[..open + 1]);
                rest = after_open;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A link is a mention only when its label is exactly the final path segment —
/// the shape Codex writes — and the target is a path rather than a URL.
fn is_mention_target(label: &str, target: &str) -> bool {
    if label.is_empty() || target.is_empty() || target.starts_with('#') {
        return false;
    }
    if target.contains(char::is_whitespace)
        || target.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        })
    {
        return false;
    }
    target.trim_end_matches('/').rsplit('/').next() == Some(label)
}

/// Project one app-server thread into a sidebar summary. Threads without a
/// `cwd` cannot be filed under a project, so they are dropped.
pub(crate) fn thread_summary_from(
    thread: &Value,
    pinned_threads: &HashSet<&str>,
) -> Option<ThreadSummary> {
    let cwd = str_at(thread, "cwd")?;
    let id = str_at(thread, "id").unwrap_or_default().to_string();
    let preview = title_text(thread).unwrap_or("Untitled thread");
    // `status` arrives either as a bare string or as a tagged object.
    let status = thread
        .get("status")
        .and_then(|status| status.as_str().or_else(|| str_at(status, "type")))
        .unwrap_or("notLoaded")
        .to_string();
    Some(ThreadSummary {
        pinned: pinned_threads.contains(id.as_str()),
        id,
        cwd: cwd.to_string(),
        title: title_from(preview),
        updated_at: i64_at(thread, "updatedAt").unwrap_or_default(),
        status,
        parent_thread_id: str_at(thread, "parentThreadId").map(str::to_string),
        agent_nickname: str_at(thread, "agentNickname").map(str::to_string),
        agent_role: str_at(thread, "agentRole").map(str::to_string),
        project_id: str_at(thread, "projectId").map(str::to_string),
        section_id: thread
            .get("section")
            .and_then(|section| str_at(section, "id"))
            .map(str::to_string),
        subagent_count: 0,
        hidden: false,
        harness: None,
    })
}

/// Build a search-index row from a raw app-server thread value. Keeps the full
/// preview text (not just the truncated title) so search can match on it.
pub(crate) fn thread_search_row(thread: &Value, archived: bool) -> Option<StoredThreadSearch> {
    let cwd = str_at(thread, "cwd")?;
    let id = str_at(thread, "id").filter(|id| !id.is_empty())?;
    Some(StoredThreadSearch {
        thread_id: id.to_string(),
        title: title_from(title_text(thread).unwrap_or("Untitled thread")),
        preview: str_at(thread, "preview")
            .or_else(|| str_at(thread, "name"))
            .unwrap_or_default()
            .chars()
            .take(PREVIEW_CHARS)
            .collect(),
        project_path: cwd.to_string(),
        updated_at: i64_at(thread, "updatedAt").unwrap_or_default(),
        archived,
    })
}

/// How many threads sit anywhere beneath each thread, so a parent can show a
/// subagent count. Walks parent links upward and stops on a cycle.
pub(crate) fn descendant_counts(threads: &[ThreadSummary]) -> HashMap<String, usize> {
    let parent_by_id: HashMap<&str, &str> = threads
        .iter()
        .filter_map(|thread| {
            thread
                .parent_thread_id
                .as_deref()
                .map(|parent| (thread.id.as_str(), parent))
        })
        .collect();
    let mut counts = HashMap::new();
    for thread in threads
        .iter()
        .filter(|thread| thread.parent_thread_id.is_some())
    {
        let mut parent = thread.parent_thread_id.as_deref();
        let mut seen = HashSet::new();
        while let Some(parent_id) = parent {
            if !seen.insert(parent_id) {
                break;
            }
            *counts.entry(parent_id.to_string()).or_default() += 1;
            parent = parent_by_id.get(parent_id).copied();
        }
    }
    counts
}

impl From<&ThreadSummary> for StoredThreadSummary {
    fn from(thread: &ThreadSummary) -> Self {
        Self {
            id: thread.id.clone(),
            cwd: thread.cwd.clone(),
            title: thread.title.clone(),
            updated_at: thread.updated_at,
            status: thread.status.clone(),
            parent_thread_id: thread.parent_thread_id.clone(),
            agent_nickname: thread.agent_nickname.clone(),
            agent_role: thread.agent_role.clone(),
            project_id: thread.project_id.clone(),
            section_id: thread.section_id.clone(),
            harness: thread.harness.clone(),
        }
    }
}

impl From<StoredThreadSummary> for ThreadSummary {
    fn from(thread: StoredThreadSummary) -> Self {
        Self {
            id: thread.id,
            cwd: thread.cwd,
            title: thread.title,
            updated_at: thread.updated_at,
            status: thread.status,
            pinned: false,
            parent_thread_id: thread.parent_thread_id,
            agent_nickname: thread.agent_nickname,
            agent_role: thread.agent_role,
            project_id: thread.project_id,
            section_id: thread.section_id,
            subagent_count: 0,
            hidden: false,
            harness: thread.harness,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn summary(id: &str, parent: Option<&str>) -> ThreadSummary {
        ThreadSummary {
            id: id.into(),
            cwd: "/project".into(),
            title: id.into(),
            updated_at: 0,
            status: "idle".into(),
            pinned: false,
            parent_thread_id: parent.map(str::to_string),
            agent_nickname: None,
            agent_role: None,
            project_id: None,
            section_id: None,
            subagent_count: 0,
            hidden: false,
            harness: None,
        }
    }

    #[test]
    fn collapses_persisted_file_mentions_in_titles() {
        assert_eq!(
            strip_mention_markup("add comments to [index.ts](packages/cli/index.ts) please"),
            "add comments to @index.ts please"
        );
        assert_eq!(
            strip_mention_markup("[a.ts](src/a.ts) vs [cli](packages/cli/)"),
            "@a.ts vs @cli"
        );
    }

    #[test]
    fn leaves_prose_links_and_stray_brackets_intact() {
        for text in [
            "see [the docs](https://example.com/docs)",
            "jump to [the plan](#plan)",
            "read [this file](src/lib/utils.ts)",
            "an array [0] and [unclosed(",
        ] {
            assert_eq!(strip_mention_markup(text), text);
        }
    }

    #[test]
    fn parses_thread_summary_fallbacks_and_structured_status() {
        let summary = thread_summary_from(
            &json!({
                "id": "thread-1",
                "cwd": "/project",
                "preview": "First line\nSecond line",
                "status": {"type": "active"}
            }),
            &HashSet::from(["thread-1"]),
        )
        .unwrap();
        assert_eq!(summary.title, "First line");
        assert_eq!(summary.status, "active");
        assert!(summary.pinned);

        let fallback = thread_summary_from(&json!({"cwd": "/project"}), &HashSet::new()).unwrap();
        assert_eq!(fallback.title, "Untitled thread");
        assert_eq!(fallback.status, "notLoaded");
        assert!(thread_summary_from(&json!({"id": "missing-cwd"}), &HashSet::new()).is_none());
    }

    #[test]
    fn search_rows_keep_the_full_preview_but_a_one_line_title() {
        let row = thread_search_row(
            &json!({
                "id": "t1",
                "cwd": "/proj",
                "name": "Fix [main.rs](src/main.rs)",
                "preview": "Fix the parser\nacross several lines",
                "updatedAt": 7,
            }),
            true,
        )
        .unwrap();
        assert_eq!(row.title, "Fix @main.rs");
        assert_eq!(row.preview, "Fix the parser\nacross several lines");
        assert_eq!(row.updated_at, 7);
        assert!(row.archived);

        // A row needs both an id and a cwd to be filed anywhere.
        assert!(thread_search_row(&json!({"cwd": "/proj", "id": ""}), false).is_none());
        assert!(thread_search_row(&json!({"id": "t1"}), false).is_none());
    }

    #[test]
    fn counts_every_descendant_depth() {
        let counts = descendant_counts(&[
            summary("root", None),
            summary("child", Some("root")),
            summary("grandchild", Some("child")),
        ]);
        assert_eq!(counts.get("root"), Some(&2));
        assert_eq!(counts.get("child"), Some(&1));
        assert_eq!(counts.get("grandchild"), None);
    }

    #[test]
    fn descendant_counting_stops_at_cycles() {
        let counts = descendant_counts(&[summary("one", Some("two")), summary("two", Some("one"))]);
        assert_eq!(counts.get("one"), Some(&2));
        assert_eq!(counts.get("two"), Some(&2));
    }
}
