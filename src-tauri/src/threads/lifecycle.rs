//! Everything that changes a thread's existence rather than its contents:
//! renaming, compacting, archiving, deleting, forking, rolling back.
//!
//! Each of these has a local-cache consequence as well as an app-server call.
//! Archiving keeps the search row (flag flipped) so history stays searchable;
//! deleting removes it.

use serde_json::{json, Value};
use tauri::{AppHandle, State};

use crate::codex::compat::{method_unsupported, Feature};
use crate::codex::requests;
use crate::harness::driver::unsupported;
use crate::harness::HarnessKind;
use crate::projects::{bootstrap_cached, bootstrap_inner, BootstrapData};
use crate::storage;
use crate::util::json::Json;
use crate::{AppState, HomeContext};

/// Refuse a Codex-only verb on a harness thread, rather than sending it to an
/// app-server that has never heard of the thread.
async fn codex_only(ctx: &HomeContext, thread_id: &str, what: &str) -> Result<(), String> {
    match ctx.harness_thread(thread_id).await? {
        Some((_, driver)) => Err(unsupported(driver.kind(), what)),
        None => Ok(()),
    }
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn rename_thread(
    thread_id: String,
    name: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let name = name.trim();
    if let Some((_, driver)) = ctx.harness_thread(&thread_id).await? {
        storage::rename_harness_thread(&ctx.database(), &thread_id, name).await?;
        driver.rename(&thread_id, name).await;
    } else {
        ctx.session
            .request(
                &app,
                "thread/name/set",
                json!({"threadId": thread_id, "name": name}),
            )
            .await?;
    }
    storage::rename_thread_summary(&ctx.database(), &thread_id, name).await?;
    storage::rename_thread_search(&ctx.database(), &thread_id, name).await?;
    storage::invalidate_thread_detail(&ctx.database(), &thread_id).await?;
    // A deliberate rename is final: it stops the auto-namer touching this thread.
    storage::write_thread_name_source(&ctx.database(), &thread_id, "user").await?;
    bootstrap_cached(&ctx).await
}

/// Ask Codex to summarise the thread so far and drop the raw history from the
/// model's context. Compaction runs as a turn, so the result streams back as
/// ordinary thread events; only the cached projection needs clearing here.
#[tauri::command]
#[specta::specta]
pub(crate) async fn compact_thread(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    // The compacting turn starts in the thread's saved mode, so any mode
    // change still buffered has to land first.
    ctx.drivers.flush().await;
    if let Some((thread, driver)) = ctx.harness_thread(&thread_id).await? {
        let resume = !storage::read_thread_items(&ctx.database(), &thread_id)
            .await?
            .is_empty();
        driver.compact(&app, &thread, resume).await?;
        return Ok(());
    }
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    storage::invalidate_thread_detail(&ctx.database(), &thread_id).await?;
    ctx.session
        .send(&app, requests::thread_compact(&thread_id))
        .await?;
    Ok(())
}

/// Start a review turn in this thread (`/review`).
///
/// `target` is the app-server's `ReviewTarget` as the picker built it —
/// `uncommittedChanges`, `baseBranch`, `commit`, or the free-form `custom` that
/// `/review <instructions>` sends — and is forwarded untouched. It falls back to
/// the uncommitted changes when the caller names no target at all. Like
/// compaction this runs as a turn, so results stream back as ordinary thread
/// events.
///
/// The response's turn is handed back rather than dropped: a review emits no
/// `turn/started`, so this is the only place the app learns the turn's id, and
/// Stop needs it to name the turn it is interrupting.
#[tauri::command]
#[specta::specta]
pub(crate) async fn start_review(
    thread_id: String,
    target: Option<Json>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    codex_only(&ctx, &thread_id, "reviews").await?;
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    let target = target
        .map(|target| target.0)
        .filter(|target| target.is_object())
        .unwrap_or_else(|| json!({"type": "uncommittedChanges"}));
    let response = ctx
        .session
        .request(
            &app,
            "review/start",
            json!({"threadId": thread_id, "target": target}),
        )
        .await?;
    response
        .get("turn")
        .cloned()
        .map(Json)
        .ok_or_else(|| "Codex returned no turn data".to_string())
}

/// Set or update the goal for a long-running task (`/goal <objective>`).
/// Only the fields given change; the app-server keeps the rest of the goal.
/// `token_budget` caps the goal's spend; `clear_token_budget` lifts the cap.
/// (Two flat parameters rather than a nested `Option` so the binding stays
/// simple; both given means clear.)
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn thread_goal_set(
    thread_id: String,
    objective: Option<String>,
    status: Option<String>,
    token_budget: Option<i64>,
    clear_token_budget: Option<bool>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    codex_only(&ctx, &thread_id, "goals").await?;
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    let budget = if clear_token_budget.unwrap_or(false) {
        Some(None)
    } else {
        token_budget.map(Some)
    };
    let request =
        requests::thread_goal_set(&thread_id, objective.as_deref(), status.as_deref(), budget);
    let response = ctx
        .session
        .request(&app, request.method, request.params)
        .await?;
    response
        .get("goal")
        .cloned()
        .map(Json)
        .ok_or_else(|| "Codex returned no goal".to_string())
}

/// Read the thread's goal, if one is set (`/goal` with no argument).
#[tauri::command]
#[specta::specta]
pub(crate) async fn thread_goal_get(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    if ctx.harness_thread(&thread_id).await?.is_some() {
        return Ok(Json(Value::Null));
    }
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    let request = requests::thread_goal_get(&thread_id);
    let response = ctx
        .session
        .request(&app, request.method, request.params)
        .await?;
    Ok(Json(response.get("goal").cloned().unwrap_or(Value::Null)))
}

/// Drop the thread's goal (`/goal clear`).
#[tauri::command]
#[specta::specta]
pub(crate) async fn thread_goal_clear(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    codex_only(&ctx, &thread_id, "goals").await?;
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    let request = requests::thread_goal_clear(&thread_id);
    ctx.session
        .request(&app, request.method, request.params)
        .await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn invalidate_thread_cache(
    thread_id: String,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let ctx = state.ctx(&window);
    storage::invalidate_thread_detail(&ctx.database(), &thread_id).await
}

/// Drop a thread from the sidebar's local state. Shared by archive and delete,
/// which differ only in what they do to the search index.
async fn remove_thread_locally(ctx: &crate::HomeContext, thread_id: &str) -> Result<(), String> {
    storage::update_store(&ctx.database(), |store| {
        store.pinned_threads.retain(|id| id != thread_id);
        store.hidden_threads.retain(|id| id != thread_id);
    })
    .await?;
    storage::delete_thread_summary(&ctx.database(), thread_id).await?;
    storage::invalidate_thread_detail(&ctx.database(), thread_id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn archive_thread(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    if let Some((_, driver)) = ctx.harness_thread(&thread_id).await? {
        driver.close_thread(&thread_id);
        storage::set_harness_thread_archived(&ctx.database(), &thread_id, true).await?;
    } else {
        ctx.session
            .send(&app, requests::thread_archive(&thread_id))
            .await?;
    }
    // Archiving keeps the thread searchable; the search row flips its flag
    // instead of being deleted.
    storage::set_thread_search_archived(&ctx.database(), &thread_id, true).await?;
    remove_thread_locally(&ctx, &thread_id).await?;
    bootstrap_cached(&ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn unarchive_thread(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    if ctx.harness_thread(&thread_id).await?.is_some() {
        storage::set_harness_thread_archived(&ctx.database(), &thread_id, false).await?;
    } else {
        ctx.session
            .send(&app, requests::thread_unarchive(&thread_id))
            .await?;
    }
    storage::set_thread_search_archived(&ctx.database(), &thread_id, false).await?;
    // A full bootstrap, not a cached one: the thread has to come back from the
    // app-server's active listing before it can reappear in the sidebar.
    bootstrap_inner(&app, &ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_thread(
    thread_id: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    // Message-version branches are hidden threads only reachable through
    // their parent, so they go with it rather than lingering as orphans.
    for branch_id in storage::branch_descendants(&ctx.database(), &thread_id).await? {
        delete_one_thread(&app, &ctx, &branch_id).await?;
    }
    delete_one_thread(&app, &ctx, &thread_id).await?;
    bootstrap_cached(&ctx).await
}

async fn delete_one_thread(
    app: &AppHandle,
    ctx: &crate::HomeContext,
    thread_id: &str,
) -> Result<(), String> {
    if let Some((thread, driver)) = ctx.harness_thread(thread_id).await? {
        driver.close_thread(thread_id);
        driver.forget_thread(&thread).await;
        storage::delete_harness_thread(&ctx.database(), thread_id).await?;
    } else {
        ctx.session
            .send(app, requests::thread_delete(thread_id))
            .await?;
    }
    storage::delete_thread_search(&ctx.database(), thread_id).await?;
    storage::delete_tip_snapshots(&ctx.database(), thread_id).await?;
    // Archiving keeps the journal (unarchiving expects its transcript back);
    // deleting is the one path that owns dropping it.
    storage::delete_thread_items(&ctx.database(), thread_id).await?;
    storage::delete_turn_settings(&ctx.database(), thread_id).await?;
    storage::delete_thread_branch(&ctx.database(), thread_id).await?;
    remove_thread_locally(ctx, thread_id).await?;
    storage::unassign_thread_workspace(&ctx.database(), thread_id).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn list_models(
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .send(&app, requests::model_list(100, true))
        .await
        .map(Json)
}

/// Drop the last `num_turns` turns from a thread, in place. Unlike forking,
/// this keeps the thread id — which is what editing a past message wants: the
/// conversation rewinds instead of branching into a second sidebar entry.
/// The models a harness offers the composer. Codex answers `model/list`;
/// every other driver builds the same shape from what its CLI offers.
#[tauri::command]
#[specta::specta]
pub(crate) async fn list_harness_models(
    harness: String,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let kind = HarnessKind::parse(&harness).ok_or_else(|| format!("Unknown harness: {harness}"))?;
    let ctx = state.ctx(&window);
    if let Some(driver) = ctx.drivers.get(kind) {
        return driver.models().await.map(Json);
    }
    // The same listing as `list_models`, so the composer can ask for any
    // harness's models through this one command.
    ctx.session
        .send(&app, requests::model_list(100, true))
        .await
        .map(Json)
}

/// Replace the thread's durable history with the prefix before `before_turn_id`
/// (`thread/revert`). The response carries no turns, so the caller supplies
/// `kept_turn_ids` for journal pruning.
#[tauri::command]
#[specta::specta]
pub(crate) async fn revert_thread(
    thread_id: String,
    before_turn_id: String,
    kept_turn_ids: Vec<String>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    codex_only(&ctx, &thread_id, "undo").await?;
    ctx.session.ensure_resumed(&app, &thread_id).await?;
    // Codex 0.146.0 has no `thread/revert`, and every version refuses it for
    // threads outside paginated history mode (created before 0.152). Both
    // come back under `codex-revert-unsupported`, which the frontend shows as
    // "undo isn't available". Only the first is cached: the second is per
    // thread.
    let response = if let Some(reason) = ctx.session.unsupported(&app, Feature::REVERT).await? {
        return Err(Feature::REVERT.error(&reason));
    } else {
        match ctx
            .session
            .send(&app, requests::thread_revert(&thread_id, &before_turn_id))
            .await
        {
            Ok(response) => response,
            Err(error) if error.contains(requests::REVERT_NEEDS_PAGINATED) => {
                return Err(Feature::REVERT.error("this thread's history format cannot be rewound"));
            }
            Err(error) => match method_unsupported(&error, Feature::REVERT.method_prefix) {
                Some(reason) => {
                    ctx.session
                        .mark_unsupported(&app, Feature::REVERT, &reason)
                        .await?;
                    return Err(Feature::REVERT.error(&reason));
                }
                None => return Err(error),
            },
        }
    };
    storage::invalidate_thread_detail(&ctx.database(), &thread_id).await?;
    storage::retain_thread_turns(&ctx.database(), &thread_id, &kept_turn_ids).await?;
    storage::retain_turn_settings(&ctx.database(), &thread_id, &kept_turn_ids).await?;
    crate::usage::ledger::send(
        &app,
        &ctx.home_key,
        crate::usage::ledger::LedgerEvent::Reset {
            thread_id: thread_id.clone(),
        },
    );
    Ok(Json(response))
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn fork_thread(
    thread_id: String,
    before_turn_id: Option<String>,
    last_turn_id: Option<String>,
    cwd: Option<String>,
    app: AppHandle,
    window: crate::HomeWindow,
    state: State<'_, AppState>,
) -> Result<Json, String> {
    let ctx = state.ctx(&window);
    if let Some((_, driver)) = ctx.harness_thread(&thread_id).await? {
        return Err(unsupported(driver.kind(), "forking"));
    }
    let mut params = json!({"threadId": thread_id});
    // An explicit cwd is the existing "Move to worktree" operation. It must
    // leave the virtual workspace instead of having its next turn overridden
    // back to the workspace hub.
    let inherits_workspace = cwd.as_deref().is_none_or(|cwd| cwd.trim().is_empty());
    if let Some(before_turn_id) = before_turn_id {
        params["beforeTurnId"] = json!(before_turn_id);
    }
    if let Some(last_turn_id) = last_turn_id {
        params["lastTurnId"] = json!(last_turn_id);
    }
    // "Move to worktree" forks the thread onto a new working directory. The
    // fork carries the full history (whose turns keep their original cwd) while
    // subsequent turns run in `cwd`.
    if let Some(cwd) = cwd.filter(|cwd| !cwd.trim().is_empty()) {
        params["cwd"] = json!(cwd);
    }
    let response = ctx.session.request(&app, "thread/fork", params).await?;
    let thread = response
        .get("thread")
        .cloned()
        .ok_or_else(|| "Codex returned no forked thread".to_string())?;
    if let Some(id) = crate::util::json::str_at(&thread, "id") {
        ctx.session.mark_resumed(&app, id, &response).await?;
        // The fork carries the parent's history, so it needs the parent's
        // journal too — otherwise the copy loses every command that ran.
        storage::copy_thread_items(&ctx.database(), &thread_id, id).await?;
        storage::copy_turn_settings(&ctx.database(), &thread_id, id).await?;
        if inherits_workspace {
            if let Some(workspace_id) =
                storage::workspace_for_thread(&ctx.database(), &thread_id).await?
            {
                storage::assign_thread_workspace(&ctx.database(), id, &workspace_id).await?;
                crate::projects::server::assign_thread_to_workspace(&app, &ctx, id, &workspace_id)
                    .await?;
            }
        }
    }
    Ok(Json(super::read::with_thread_settings(
        thread,
        Some(&response),
    )))
}
