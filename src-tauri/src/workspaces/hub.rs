//! The hub directory: one symlink per member, plus a manifest.
//!
//! The hub is intentionally user-writable — it is where notes and plans that
//! span the whole workspace live. So every operation here only creates, repairs,
//! or removes links it can *prove* are its own; anything else is left alone and
//! reported as a conflict rather than overwritten.

use serde_json::json;
use std::fs;
use std::path::Path;

use crate::storage::{StoredWorkspace, StoredWorkspaceMember};

/// The legacy metadata subdirectory Pingex continues to own inside the hub.
pub(crate) const METADATA_DIR: &str = ".pingu";

#[cfg(unix)]
fn link_directory(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn link_directory(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

/// Whether `link` is a symlink resolving to exactly `target` — the proof that
/// a hub entry is one Pingex created rather than a user's own file.
pub(crate) fn link_matches(link: &Path, target: &Path) -> bool {
    fs::read_link(link)
        .ok()
        .and_then(|value| fs::canonicalize(value).ok())
        .zip(fs::canonicalize(target).ok())
        .is_some_and(|(actual, expected)| actual == expected)
}

/// Create or repair the hub for `workspace`. Idempotent: an existing correct
/// link is left alone, and a conflicting entry fails rather than clobbering.
pub(crate) fn materialize_hub(
    workspace: &StoredWorkspace,
    members: &[StoredWorkspaceMember],
) -> Result<(), String> {
    let hub = Path::new(&workspace.hub_path);
    fs::create_dir_all(hub)
        .map_err(|error| format!("Could not create workspace directory: {error}"))?;
    let metadata = hub.join(METADATA_DIR);
    fs::create_dir_all(&metadata)
        .map_err(|error| format!("Could not create workspace metadata directory: {error}"))?;
    for member in members {
        let link = hub.join(&member.alias);
        let target = Path::new(&member.effective_path);
        if link.exists() || fs::symlink_metadata(&link).is_ok() {
            if !link_matches(&link, target) {
                return Err(format!(
                    "Workspace alias '{}' already exists and is not the managed project link",
                    member.alias
                ));
            }
            continue;
        }
        link_directory(target, &link).map_err(|error| {
            format!(
                "Could not link workspace member '{}': {error}",
                member.alias
            )
        })?;
    }
    let manifest = json!({
        "workspaceId": workspace.id,
        "name": workspace.name,
        "members": members.iter().map(|member| json!({
            "alias": member.alias,
            "sourcePath": member.source_path,
            "effectivePath": member.effective_path,
            "isolated": member.isolated,
            "branch": member.branch,
        })).collect::<Vec<_>>(),
    });
    fs::write(
        metadata.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Could not write workspace manifest: {error}"))
}

/// Remove one member's link, but only if it is still the link Pingex made.
pub(crate) fn remove_managed_link(
    hub: &Path,
    member: &StoredWorkspaceMember,
) -> Result<(), String> {
    let link = hub.join(&member.alias);
    if fs::symlink_metadata(&link).is_err() {
        return Ok(());
    }
    if !link_matches(&link, Path::new(&member.effective_path)) {
        return Err(format!(
            "Workspace alias '{}' was changed outside Pingex; it will not be removed",
            member.alias
        ));
    }
    fs::remove_file(&link).map_err(|error| {
        format!(
            "Could not update workspace alias '{}': {error}",
            member.alias
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(workspace_id: &str, alias: &str, path: &Path) -> StoredWorkspaceMember {
        StoredWorkspaceMember {
            workspace_id: workspace_id.into(),
            source_path: path.display().to_string(),
            effective_path: path.display().to_string(),
            alias: alias.into(),
            isolated: false,
            branch: None,
            ordinal: 0,
        }
    }

    #[cfg(unix)]
    #[test]
    fn hub_repair_keeps_user_notes_and_managed_links() {
        let temp = tempfile::tempdir().unwrap();
        let source_one = temp.path().join("one");
        let source_two = temp.path().join("two");
        fs::create_dir_all(&source_one).unwrap();
        fs::create_dir_all(&source_two).unwrap();
        let workspace = StoredWorkspace {
            id: "workspace-test".into(),
            name: "Test workspace".into(),
            hub_path: temp.path().join("hub").display().to_string(),
            archived: false,
        };
        let members = vec![
            member(&workspace.id, "one", &source_one),
            member(&workspace.id, "two", &source_two),
        ];
        materialize_hub(&workspace, &members).unwrap();
        let note = Path::new(&workspace.hub_path).join("NOTES.md");
        fs::write(&note, "keep this").unwrap();
        materialize_hub(&workspace, &members).unwrap();
        assert_eq!(fs::read_to_string(note).unwrap(), "keep this");
        assert!(link_matches(
            &Path::new(&workspace.hub_path).join("one"),
            &source_one
        ));
    }

    #[cfg(unix)]
    #[test]
    fn a_user_file_occupying_an_alias_is_a_conflict_not_an_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("api");
        let hub = temp.path().join("hub");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&hub).unwrap();
        fs::write(hub.join("api"), "a real file the user made").unwrap();

        let workspace = StoredWorkspace {
            id: "w1".into(),
            name: "W".into(),
            hub_path: hub.display().to_string(),
            archived: false,
        };
        let members = vec![member("w1", "api", &source)];
        assert!(materialize_hub(&workspace, &members).is_err());
        // The user's file survives untouched.
        assert_eq!(
            fs::read_to_string(hub.join("api")).unwrap(),
            "a real file the user made"
        );
        // And it is refused for removal too.
        assert!(remove_managed_link(&hub, &members[0]).is_err());
    }

    #[test]
    fn removing_an_absent_link_is_not_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let hub = temp.path().join("hub");
        fs::create_dir_all(&hub).unwrap();
        assert!(remove_managed_link(&hub, &member("w1", "gone", &temp.path().join("x"))).is_ok());
    }
}
