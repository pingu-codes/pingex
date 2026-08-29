//! Generating a short sidebar title for a thread.
//!
//! Without this, a thread's title is the first line of its first message
//! (`projects::summary::title_text`), which reads as a truncated prompt. Here a
//! throwaway Codex thread is asked to summarise the conversation in a few words,
//! and the answer is written to the thread's `name` — which wins over the
//! preview everywhere the title is derived.
//!
//! Two passes run per thread: one from the opening message alone, and one once
//! the first reply has landed and there is something to summarise. An explicit
//! rename stops both, permanently.
//!
//! Everything here is best-effort. Naming is a cosmetic nicety layered on top of
//! a turn the user actually cares about, so no failure is ever surfaced: the
//! command returns `Ok(None)` and the existing title stands.

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use crate::codex::requests;
use crate::projects::{bootstrap_cached, strip_mention_markup, BootstrapData};
use crate::settings::prefs;
use crate::storage;
use crate::util::json::{arr_or_empty, str_at};
use crate::AppState;

/// Instructions for the naming thread. It never sees the project's own
/// instructions or tools — it reads text and answers with a phrase.
pub const NAMER_INSTRUCTIONS: &str = "\
You name conversations. You are given the opening of a chat between a user and \
a coding agent. Reply with a title for it and nothing else.

Rules:
- 2 to 6 words, under 50 characters.
- Describe the task or topic, not the participants.
- Sentence case. No quotes, no trailing full stop, no prefix like \"Title:\".
- Never ask a question or explain yourself.";

/// How much of the conversation the namer is shown. Enough for the gist of a
/// long opening message; short enough to stay a cheap call.
const SEED_CHARS: usize = 2000;
/// Titles longer than this are treated as the model ignoring its instructions
/// (usually a sentence of explanation) and discarded.
const MAX_TITLE_CHARS: usize = 60;
/// How long to wait for the naming turn's answer before giving up.
const NAME_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// How often to re-read the naming thread while waiting for its reply.
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(400);

/// Generate and apply a title for `thread_id`.
///
/// `seed` is the user's opening message for the first pass; `None` asks for the
/// second pass, which reads the thread itself so the reply informs the title.
/// Returns the refreshed sidebar data when a title was applied, `None` when
/// naming was skipped or did not work out.
#[tauri::command]
#[specta::specta]
pub(crate) async fn auto_name_thread(
    thread_id: String,
    seed: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<Option<BootstrapData>, String> {
    let ctx = state.ctx(&window);
    let settings = prefs::read_auto_naming(&prefs::settings_path());
    if !settings.enabled {
        return Ok(None);
    }
    // A rename is the user's decision and outranks anything generated here.
    if storage::read_thread_name_source(&ctx.database(), &thread_id)
        .await?
        .as_deref()
        == Some("user")
    {
        return Ok(None);
    }

    // The namer runs on Codex; a thread on another harness keeps the title
    // its opening message gave it.
    if storage::thread_harness(&ctx.database(), &thread_id).await?.is_some() {
        return Ok(None);
    }
    let Some(seed) = resolve_seed(seed, &thread_id, &app, &ctx).await else {
        return Ok(None);
    };
    let Some(title) = generate_title(&seed, settings.model.as_deref(), &app, &ctx).await else {
        return Ok(None);
    };
    apply_title(&thread_id, &title, &app, &ctx).await?;
    bootstrap_cached(&ctx).await.map(Some)
}

/// The text the namer is shown: the caller's message on the first pass, or the
/// opening exchange read back from the thread on the second.
async fn resolve_seed(
    seed: Option<String>,
    thread_id: &str,
    app: &AppHandle,
    ctx: &crate::HomeContext,
) -> Option<String> {
    let raw = match seed {
        Some(text) => text,
        None => {
            let response = ctx
                .session
                .request(
                    app,
                    "thread/read",
                    json!({"threadId": thread_id, "includeTurns": true}),
                )
                .await
                .ok()?;
            opening_exchange(response.get("thread")?)
        }
    };
    let trimmed = strip_mention_markup(raw.trim());
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(SEED_CHARS).collect())
}

/// The first user message and the first agent reply of a thread, which is as
/// much as a title needs. Either may be missing from a thread that is still
/// early in its first turn.
fn opening_exchange(thread: &Value) -> String {
    let items: Vec<&Value> = arr_or_empty(thread, "turns")
        .iter()
        .flat_map(|turn| arr_or_empty(turn, "items"))
        .collect();
    let first_of = |kind: &str| {
        items
            .iter()
            .find(|item| str_at(item, "type") == Some(kind))
            .and_then(|item| str_at(item, "text"))
            .unwrap_or_default()
    };
    format!(
        "{}\n\n{}",
        first_of("userMessage"),
        first_of("agentMessage")
    )
}

/// Run the naming turn in a throwaway thread and return its answer.
///
/// The thread is started directly rather than through `threads::turn::start_thread`
/// so it inherits neither the project's instructions nor its workspace roots,
/// and it is deleted on every path so it never reaches the sidebar.
async fn generate_title(
    seed: &str,
    model: Option<&str>,
    app: &AppHandle,
    ctx: &crate::HomeContext,
) -> Option<String> {
    let started = ctx
        .session
        .send(app, requests::namer_thread_start(NAMER_INSTRUCTIONS))
        .await
        .ok()?;
    let namer_id = str_at(started.get("thread")?, "id")?.to_string();

    let title = run_naming_turn(&namer_id, seed, model, app, ctx).await;
    let _ = ctx
        .session
        .send(app, requests::thread_delete(&namer_id))
        .await;
    title
}

async fn run_naming_turn(
    namer_id: &str,
    seed: &str,
    model: Option<&str>,
    app: &AppHandle,
    ctx: &crate::HomeContext,
) -> Option<String> {
    let response = ctx
        .session
        .send(app, requests::naming_turn(namer_id, seed, model))
        .await
        .ok()?;

    // `turn/start` may answer with the finished turn or only acknowledge it,
    // depending on the app-server version, so take the reply from the response
    // when it is there and otherwise wait for the thread to carry it.
    if let Some(title) = response.get("turn").and_then(reply_text).and_then(sanitize) {
        return Some(title);
    }
    await_reply(namer_id, app, ctx).await.and_then(sanitize)
}

/// Poll the naming thread until its reply appears or the deadline passes.
async fn await_reply(
    namer_id: &str,
    app: &AppHandle,
    ctx: &crate::HomeContext,
) -> Option<String> {
    let deadline = tokio::time::Instant::now() + NAME_TIMEOUT;
    while tokio::time::Instant::now() < deadline {
        tokio::time::sleep(POLL_INTERVAL).await;
        // A transient failure (e.g. the rollout file not yet flushed) just
        // costs a poll tick; the deadline bounds how long errors can persist.
        let Ok(response) = ctx
            .session
            .send(app, requests::thread_read(namer_id))
            .await
        else {
            continue;
        };
        if let Some(text) = response.get("thread").and_then(reply_text) {
            return Some(text);
        }
    }
    None
}

/// The last agent message in a turn or thread payload, whichever shape the
/// caller passes: both carry their messages under `turns[].items[]`, and a turn
/// additionally has a bare `items[]`.
fn reply_text(value: &Value) -> Option<String> {
    let nested = arr_or_empty(value, "turns")
        .iter()
        .flat_map(|turn| arr_or_empty(turn, "items"));
    arr_or_empty(value, "items")
        .iter()
        .chain(nested)
        .filter(|item| str_at(item, "type") == Some("agentMessage"))
        .filter_map(|item| str_at(item, "text"))
        .rfind(|text| !text.trim().is_empty())
        .map(str::to_string)
}

/// Reduce a model reply to a usable title, or reject it.
///
/// Models reliably answer with the phrase alone, but not always: a stray
/// preamble line, wrapping quotes or a trailing full stop all show up. Anything
/// still too long after cleanup is an explanation rather than a title, and
/// keeping the existing title beats showing a sentence.
fn sanitize(reply: String) -> Option<String> {
    let line = reply
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .trim_start_matches("Title:")
        .trim_matches(|character: char| {
            character.is_whitespace() || matches!(character, '"' | '\'' | '`' | '*' | '#')
        })
        .trim_end_matches(['.', '!'])
        .trim();
    if line.is_empty() || line.chars().count() > MAX_TITLE_CHARS {
        return None;
    }
    Some(line.to_string())
}

/// Write the title everywhere a rename writes it, plus the provenance marker
/// that lets a later pass overwrite this but a manual rename stop it.
async fn apply_title(
    thread_id: &str,
    title: &str,
    app: &AppHandle,
    ctx: &crate::HomeContext,
) -> Result<(), String> {
    ctx
        .session
        .request(
            app,
            "thread/name/set",
            json!({"threadId": thread_id, "name": title}),
        )
        .await?;
    storage::rename_thread_summary(&ctx.database(), thread_id, title).await?;
    storage::rename_thread_search(&ctx.database(), thread_id, title).await?;
    storage::invalidate_thread_detail(&ctx.database(), thread_id).await?;
    storage::write_thread_name_source(&ctx.database(), thread_id, "auto").await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_plain_title() {
        assert_eq!(
            sanitize("Refactor sidebar grouping".into()),
            Some("Refactor sidebar grouping".into())
        );
    }

    #[test]
    fn strips_the_decoration_models_add() {
        for reply in [
            "\"Refactor sidebar grouping\"",
            "Title: Refactor sidebar grouping",
            "**Refactor sidebar grouping**",
            "Refactor sidebar grouping.",
            "\n\n  Refactor sidebar grouping  \n",
        ] {
            assert_eq!(
                sanitize(reply.into()),
                Some("Refactor sidebar grouping".into()),
                "{reply}"
            );
        }
    }

    #[test]
    fn keeps_only_the_first_line() {
        assert_eq!(
            sanitize("Fix the login bug\n\nLet me know if you want another.".into()),
            Some("Fix the login bug".into())
        );
    }

    #[test]
    fn rejects_empty_and_over_long_replies() {
        assert_eq!(sanitize(String::new()), None);
        assert_eq!(sanitize("\"\"".into()), None);
        assert_eq!(
            sanitize(
                "I would suggest naming this conversation something along the lines of a \
                 sidebar refactor"
                    .into()
            ),
            None
        );
    }

    #[test]
    fn reads_the_last_agent_message_from_either_shape() {
        let turn = json!({"items": [
            {"type": "userMessage", "text": "name this"},
            {"type": "agentMessage", "text": "Sidebar grouping refactor"},
        ]});
        assert_eq!(
            reply_text(&turn).as_deref(),
            Some("Sidebar grouping refactor")
        );

        let thread = json!({"turns": [{"items": [
            {"type": "agentMessage", "text": "first"},
            {"type": "agentMessage", "text": "second"},
        ]}]});
        assert_eq!(reply_text(&thread).as_deref(), Some("second"));
    }

    #[test]
    fn a_thread_with_no_reply_yet_has_no_text() {
        let thread = json!({"turns": [{"items": [
            {"type": "userMessage", "text": "name this"},
            {"type": "agentMessage", "text": "   "},
        ]}]});
        assert_eq!(reply_text(&thread), None);
        assert_eq!(reply_text(&json!({"id": "t1"})), None);
    }

    #[test]
    fn the_opening_exchange_is_the_first_message_of_each_side() {
        let thread = json!({"turns": [
            {"items": [
                {"type": "userMessage", "text": "fix the login bug"},
                {"type": "commandExecution", "text": "cargo test"},
                {"type": "agentMessage", "text": "Found it in auth.rs"},
            ]},
            {"items": [{"type": "userMessage", "text": "now ship it"}]},
        ]});
        assert_eq!(
            opening_exchange(&thread),
            "fix the login bug\n\nFound it in auth.rs"
        );
    }

    #[test]
    fn an_exchange_with_no_reply_yet_is_still_usable() {
        let thread = json!({"turns": [{"items": [
            {"type": "userMessage", "text": "fix the login bug"},
        ]}]});
        assert_eq!(opening_exchange(&thread), "fix the login bug\n\n");
    }
}
