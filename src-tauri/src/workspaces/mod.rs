//! Durable multi-project workspaces.
//!
//! The database owns membership. The hub directory is intentionally user
//! writable: we only create or repair member links that we can prove are ours,
//! leaving notes, plans, and user-created `AGENTS.md` files untouched.

use serde::Serialize;
use std::path::Path;

use crate::storage;
use crate::util::id::unique_suffix;
use crate::HomeContext;

pub(crate) mod commands;
mod hub;
mod worktree;

use hub::{materialize_hub, METADATA_DIR};

/// What a turn needs to run inside a workspace: where to start, which roots the
/// sandbox may touch, and the prose describing the layout to the model.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorkspaceRuntime {
    pub(crate) workspace_id: String,
    pub(crate) cwd: String,
    pub(crate) roots: Vec<String>,
    pub(crate) context: String,
}

fn workspace_id() -> String {
    format!("workspace-{}", unique_suffix())
}

/// A member alias becomes a directory name inside the hub, so it must not
/// escape it or collide with the metadata directory.
fn clean_alias(alias: &str) -> Result<String, String> {
    let alias = alias.trim();
    if alias.is_empty() || alias == "." || alias == ".." || alias == METADATA_DIR {
        return Err("Each workspace member needs a non-reserved alias".into());
    }
    if alias.contains('/') || alias.contains('\\') || alias.contains('\0') {
        return Err("Workspace aliases cannot contain path separators".into());
    }
    Ok(alias.to_string())
}

/// Resolves the workspace on every turn so membership updates are sticky for
/// old threads too. This deliberately returns the hub as a root: it contains
/// user-owned notes and plans in addition to project links.
pub(crate) async fn runtime_for_workspace(
    ctx: &HomeContext,
    workspace_id: &str,
) -> Result<WorkspaceRuntime, String> {
    let workspace = storage::read_workspaces(&ctx.database())
        .await?
        .into_iter()
        .find(|workspace| workspace.id == workspace_id && !workspace.archived)
        .ok_or("This workspace no longer exists or is archived")?;
    let members = storage::read_workspace_members(&ctx.database(), workspace_id).await?;
    if members.len() < 2 {
        return Err("A workspace needs at least two members".into());
    }
    if let Some(member) = members
        .iter()
        .find(|member| !Path::new(&member.effective_path).is_dir())
    {
        return Err(format!(
            "Workspace member '{}' is unavailable",
            member.alias
        ));
    }
    let workspace_for_disk = workspace.clone();
    let members_for_disk = members.clone();
    tauri::async_runtime::spawn_blocking(move || {
        materialize_hub(&workspace_for_disk, &members_for_disk)
    })
    .await
    .map_err(|_| "Could not prepare workspace directory".to_string())??;
    let context = format!(
        "Pingex workspace `{}`. You start in {}. Members: {}. The parent directory is writable for shared notes and plans. Each member alias maps to its effective project root; inspect that member's repository instructions before editing it.",
        workspace.name,
        workspace.hub_path,
        members
            .iter()
            .map(|member| format!("{} -> {}{}", member.alias, member.effective_path, member.branch.as_deref().map(|branch| format!(" ({branch})")).unwrap_or_default()))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut roots = vec![workspace.hub_path.clone()];
    roots.extend(members.iter().map(|member| member.effective_path.clone()));
    Ok(WorkspaceRuntime {
        workspace_id: workspace.id,
        cwd: workspace.hub_path,
        roots,
        context,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aliases_must_be_safe_and_unreserved() {
        assert_eq!(clean_alias("  api  ").unwrap(), "api");
        assert!(clean_alias("").is_err());
        assert!(clean_alias(".").is_err());
        assert!(clean_alias("..").is_err());
        assert!(clean_alias(METADATA_DIR).is_err());
        assert!(clean_alias("a/b").is_err());
        assert!(clean_alias("a\\b").is_err());
    }

    #[test]
    fn workspace_ids_are_unique() {
        let one = workspace_id();
        let two = workspace_id();
        assert_ne!(one, two);
        assert!(one.starts_with("workspace-"));
    }
}
