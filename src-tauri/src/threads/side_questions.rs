//! Side questions: throwaway threads spawned from a parent thread and shown
//! under it, rather than as top-level entries in the project.

use tauri::{AppHandle, State};

use crate::projects::{bootstrap_cached, BootstrapData};
use crate::storage::native_side_questions::{self as native, SideExchange};
use crate::storage::{self, SideQuestion};
use crate::util::time::unix_secs;
use crate::AppState;

/// Side-question titles are one-line labels in the sidebar, so they are capped.
pub const MAX_TITLE_CHARS: usize = 120;

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_side_question(
    parent_thread_id: String,
    side_thread_id: String,
    title: String,
    inherited_turns: Option<u32>,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    storage::add_side_question(
        &ctx.database(),
        &SideQuestion {
            kind: storage::SideQuestionKind::Fork,
            side_thread_id,
            parent_thread_id,
            title: title.trim().chars().take(MAX_TITLE_CHARS).collect(),
            created_at: unix_secs(),
            inherited_turns,
        },
    )
    .await?;
    bootstrap_cached(&ctx).await
}

/// Stop tracking a thread as a side question. The thread itself survives and
/// reappears as an ordinary thread in its project.
#[tauri::command]
#[specta::specta]
pub(crate) async fn remove_side_question(
    side_thread_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    storage::delete_side_question(&ctx.database(), &side_thread_id).await?;
    for exchange in native::read(&ctx.database(), &side_thread_id).await? {
        ctx.drivers
            .claude()
            .side_questions
            .cancel(&side_thread_id, &exchange.id);
    }
    native::delete(&ctx.database(), &side_thread_id).await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn side_question_mode(
    parent_thread_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<storage::SideQuestionKind, String> {
    let ctx = state.ctx(&window);
    match storage::thread_harness(&ctx.database(), &parent_thread_id).await? {
        None => Ok(storage::SideQuestionKind::Fork),
        Some(thread) if thread.harness == "claude" => Ok(storage::SideQuestionKind::Native),
        _ => Err("This harness does not support side questions".into()),
    }
}

async fn parent(ctx: &crate::HomeContext, parent: &str) -> Result<storage::HarnessThread, String> {
    storage::thread_harness(&ctx.database(), parent)
        .await?
        .filter(|thread| thread.harness == "claude")
        .ok_or_else(|| "Native side questions need a Claude thread".into())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn add_native_side_question(
    parent_thread_id: String,
    side_thread_id: String,
    title: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    parent(&ctx, &parent_thread_id).await?;
    native::create(
        &ctx.database(),
        &parent_thread_id,
        &side_thread_id,
        &title
            .trim()
            .chars()
            .take(MAX_TITLE_CHARS)
            .collect::<String>(),
    )
    .await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn read_native_side_question(
    parent_thread_id: String,
    side_thread_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<SideExchange>, String> {
    let ctx = state.ctx(&window);
    native::require_owner(&ctx.database(), &parent_thread_id, &side_thread_id).await?;
    native::read(&ctx.database(), &side_thread_id).await
}

/// Persist before starting: Stop can cancel even before the answer command arrives.
#[tauri::command]
#[specta::specta]
pub(crate) async fn prepare_native_side_question(
    parent_thread_id: String,
    side_thread_id: String,
    question: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let ctx = state.ctx(&window);
    parent(&ctx, &parent_thread_id).await?;
    native::require_owner(&ctx.database(), &parent_thread_id, &side_thread_id).await?;
    if question.trim().is_empty() {
        return Err("Enter a side question".into());
    }
    let id = uuid::Uuid::new_v4().to_string();
    native::begin(&ctx.database(), &side_thread_id, &id, question.trim()).await?;
    Ok(id)
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn answer_native_side_question(
    parent_thread_id: String,
    side_thread_id: String,
    request_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<SideExchange>, String> {
    let ctx = state.ctx(&window);
    let database = ctx.database();
    native::require_owner(&database, &parent_thread_id, &side_thread_id).await?;
    let Some(operation) = begin_answer(
        &database,
        &ctx.drivers.claude().side_questions,
        &side_thread_id,
        &request_id,
    )
    .await?
    else {
        return native::read(&database, &side_thread_id).await;
    };
    let exchanges = native::read(&database, &side_thread_id).await?;
    let exchange = exchanges
        .iter()
        .find(|entry| entry.id == request_id)
        .ok_or_else(|| "Side question no longer exists".to_string())?;
    if exchange.outcome != "pending" {
        return Ok(exchanges);
    }
    let result = async {
        ctx.drivers.claude().flush_modes().await;
        let thread = parent(&ctx, &parent_thread_id).await?;
        let resume = !storage::read_thread_items(&database, &parent_thread_id)
            .await?
            .is_empty();
        let settings = storage::read_turn_settings(&database, &parent_thread_id).await?;
        ctx.drivers
            .claude()
            .ask_side_question(
                &app,
                &parent_thread_id,
                &thread.cwd,
                resume,
                settings.last(),
                &crate::claude::modes::ClaudeModeSettings::restore(
                    thread.permission_mode.as_deref(),
                    thread.plan_exit_mode.as_deref(),
                )?,
                &request_id,
                &exchange.question,
                native::history(&exchanges),
                operation.cancelled.clone(),
            )
            .await
    }
    .await;
    match result {
        Ok(answer) => {
            let response = answer.response.as_deref();
            let outcome =
                if response.is_some_and(|text| !text.trim().is_empty()) && !answer.synthetic {
                    "completed"
                } else {
                    "error"
                };
            native::finish(
                &database,
                &side_thread_id,
                &request_id,
                outcome,
                Some(response.unwrap_or("Claude returned no side-question answer. Try again.")),
                answer.synthetic,
                answer.refusal.as_ref(),
            )
            .await?;
        }
        Err(error) => {
            native::finish(
                &database,
                &side_thread_id,
                &request_id,
                if *operation.cancelled.borrow() {
                    "interrupted"
                } else {
                    "error"
                },
                Some(&error),
                false,
                None,
            )
            .await?
        }
    }
    native::read(&database, &side_thread_id).await
}

async fn begin_answer(
    database: &turso::Database,
    requests: &crate::claude::side_questions::SideRequests,
    side: &str,
    request: &str,
) -> Result<Option<crate::claude::side_questions::SideRequest>, String> {
    match requests.begin(side, request) {
        Ok(operation) => Ok(Some(operation)),
        Err(_) if requests.active(side, request) => Ok(None),
        Err(error) => {
            native::finish(database, side, request, "error", Some(&error), false, None).await?;
            Ok(None)
        }
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn cancel_native_side_question(
    parent_thread_id: String,
    side_thread_id: String,
    request_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Vec<SideExchange>, String> {
    let ctx = state.ctx(&window);
    native::require_owner(&ctx.database(), &parent_thread_id, &side_thread_id).await?;
    native::finish(
        &ctx.database(),
        &side_thread_id,
        &request_id,
        "interrupted",
        None,
        false,
        None,
    )
    .await?;
    ctx.drivers
        .claude()
        .side_questions
        .cancel(&side_thread_id, &request_id);
    native::read(&ctx.database(), &side_thread_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn follow_up_during_cancellation_never_stays_pending() {
        let dir = tempfile::tempdir().unwrap();
        let db = storage::open(dir.path()).await.unwrap();
        let requests = crate::claude::side_questions::SideRequests::default();
        native::create(&db, "parent", "side", "First")
            .await
            .unwrap();
        native::begin(&db, "side", "first", "First?").await.unwrap();
        let first = begin_answer(&db, &requests, "side", "first")
            .await
            .unwrap()
            .unwrap();
        assert!(begin_answer(&db, &requests, "side", "first")
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            native::read(&db, "side").await.unwrap()[0].outcome,
            "pending"
        );
        native::finish(&db, "side", "first", "interrupted", None, false, None)
            .await
            .unwrap();
        requests.cancel("side", "first");
        native::begin(&db, "side", "second", "Second?")
            .await
            .unwrap();
        assert!(begin_answer(&db, &requests, "side", "second")
            .await
            .unwrap()
            .is_none());
        assert_eq!(native::read(&db, "side").await.unwrap()[1].outcome, "error");
        drop(first);
        native::begin(&db, "side", "third", "Retry?").await.unwrap();
        assert!(begin_answer(&db, &requests, "side", "third")
            .await
            .unwrap()
            .is_some());
    }
}
