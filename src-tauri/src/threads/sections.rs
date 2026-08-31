//! Thread sections (`threadSection/*` and `thread/section/move`): named,
//! coloured buckets the app-server keeps for threads, stable in Codex 0.149
//! and absent from 0.146.
//!
//! Sections are global on the server, not per project; the sidebar shows
//! each one inside every project that has a thread in it. The server list is
//! cached locally on every full bootstrap (see [`sync`]) so the cached path
//! can group threads without a round trip, and the sidebar hides the feature
//! entirely once a Codex has refused the API.

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::codex::compat::Feature;
use crate::codex::requests;
use crate::projects::{bootstrap_cached, BootstrapData};
use crate::storage::{self, StoredThreadSection};
use crate::util::json::{arr_or_empty, str_at};
use crate::{AppState, HomeContext};

fn section_from(value: &Value) -> Option<StoredThreadSection> {
    let appearance = value.get("appearance");
    Some(StoredThreadSection {
        id: str_at(value, "id")?.to_string(),
        name: str_at(value, "name").unwrap_or_default().to_string(),
        icon: appearance
            .and_then(|appearance| str_at(appearance, "icon"))
            .map(str::to_string),
        color: appearance
            .and_then(|appearance| str_at(appearance, "color"))
            .map(str::to_string),
    })
}

/// Fetch the server's sections into the local cache. Returns
/// `(supported, sections)`; a refusal caches "unsupported" so the sidebar
/// stops offering sections until the next full bootstrap tries again.
pub(crate) async fn sync(
    app: &AppHandle,
    ctx: &HomeContext,
) -> Result<(bool, Vec<StoredThreadSection>), String> {
    let mut sections = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let page = match ctx
            .session
            .send_gated(
                app,
                Feature::SECTIONS,
                requests::thread_section_list(cursor.as_deref()),
                |_| None,
            )
            .await
        {
            Ok(page) => page,
            Err(error) if error.starts_with(Feature::SECTIONS.error_prefix) => {
                storage::replace_thread_sections(&ctx.database(), &[], false).await?;
                return Ok((false, Vec::new()));
            }
            Err(error) => return Err(error),
        };
        sections.extend(arr_or_empty(&page, "data").iter().filter_map(section_from));
        cursor = str_at(&page, "nextCursor").map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    storage::replace_thread_sections(&ctx.database(), &sections, true).await?;
    Ok((true, sections))
}

/// Re-read the server's sections after a mutation, then rebuild the sidebar
/// from cache. The mutation already succeeded, so a refusal here is a real
/// error rather than an "unsupported" verdict.
async fn resync(app: &AppHandle, ctx: &HomeContext) -> Result<BootstrapData, String> {
    sync(app, ctx).await?;
    bootstrap_cached(ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn create_thread_section(
    name: String,
    color: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let name = name.trim();
    if name.is_empty() {
        return Err("A section needs a name".into());
    }
    ctx.session
        .send(
            &app,
            requests::thread_section_create(name, color.as_deref()),
        )
        .await?;
    resync(&app, &ctx).await
}

#[tauri::command]
#[specta::specta]
pub(crate) async fn update_thread_section(
    section_id: String,
    name: String,
    color: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    let name = name.trim();
    if name.is_empty() {
        return Err("A section needs a name".into());
    }
    ctx.session
        .send(
            &app,
            requests::thread_section_update(&section_id, name, color.as_deref()),
        )
        .await?;
    resync(&app, &ctx).await
}

/// Delete a section. Its threads stay where they are, just unsectioned —
/// the server handles that; the cached summaries are cleared to match.
#[tauri::command]
#[specta::specta]
pub(crate) async fn delete_thread_section(
    section_id: String,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .send(&app, requests::thread_section_delete(&section_id))
        .await?;
    let summaries = storage::read_thread_summaries(&ctx.database()).await?;
    for summary in summaries
        .iter()
        .filter(|summary| summary.section_id.as_deref() == Some(section_id.as_str()))
    {
        storage::set_thread_section(&ctx.database(), &summary.id, None).await?;
    }
    resync(&app, &ctx).await
}

/// Move a thread into `section_id`, or out of its section when `None`.
#[tauri::command]
#[specta::specta]
pub(crate) async fn move_thread_to_section(
    thread_id: String,
    section_id: Option<String>,
    app: AppHandle,
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
) -> Result<BootstrapData, String> {
    let ctx = state.ctx(&window);
    ctx.session
        .send(
            &app,
            requests::thread_section_move(&thread_id, section_id.as_deref()),
        )
        .await?;
    storage::set_thread_section(&ctx.database(), &thread_id, section_id.as_deref()).await?;
    bootstrap_cached(&ctx).await
}

#[cfg(test)]
mod tests {
    use super::section_from;
    use serde_json::json;

    #[test]
    fn reads_a_section_with_or_without_an_appearance() {
        let coloured = section_from(&json!({
            "id": "s1", "name": "Bugs", "appearance": {"icon": null, "color": "#ef4444"}
        }))
        .unwrap();
        assert_eq!(coloured.color.as_deref(), Some("#ef4444"));
        assert_eq!(coloured.icon, None);

        let plain =
            section_from(&json!({"id": "s2", "name": "Later", "appearance": null})).unwrap();
        assert_eq!(plain.color, None);
        assert!(section_from(&json!({"name": "no id"})).is_none());
    }
}
