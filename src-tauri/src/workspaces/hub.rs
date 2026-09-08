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
use crate::util::host::Host;

/// The legacy metadata subdirectory Pingex continues to own inside the hub.
pub(crate) const METADATA_DIR: &str = ".pingu";

#[cfg(unix)]
fn link_directory_native(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn link_directory_native(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

/// Create the symlink `link` → `target` on `host`. Both are host paths. A
/// Linux symlink cannot be made through the `\\wsl.localhost` share, so a
/// WSL host runs `ln` inside the distribution.
fn link_directory(host: &Host, target: &str, link: &str) -> Result<(), String> {
    match host {
        Host::Native => link_directory_native(Path::new(target), Path::new(link))
            .map_err(|error| error.to_string()),
        Host::Wsl { .. } => {
            let status = host
                .command("ln", &["-s", "--", target, link], None, &[], &[])
                .status()
                .map_err(|error| error.to_string())?;
            if status.success() {
                Ok(())
            } else {
                Err("ln failed inside the distribution".to_string())
            }
        }
    }
}

/// What `link` points at, when it is a symlink at all.
fn link_target(host: &Host, link: &str) -> Option<String> {
    match host {
        Host::Native => fs::read_link(link)
            .ok()
            .map(|target| target.to_string_lossy().into_owned()),
        Host::Wsl { .. } => host
            .command("readlink", &["--", link], None, &[], &[])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|target| !target.is_empty()),
    }
}

/// Whether anything (file, folder or symlink, dangling or not) sits at `path`.
fn entry_exists(host: &Host, path: &str) -> bool {
    match host {
        Host::Native => fs::symlink_metadata(path).is_ok(),
        // The share hides dangling Linux symlinks, so ask the distribution.
        Host::Wsl { .. } => link_target(host, path).is_some() || host.to_local(path).exists(),
    }
}

/// Whether `link` is a symlink resolving to exactly `target` — the proof that
/// a hub entry is one Pingex created rather than a user's own file.
pub(crate) fn link_matches(host: &Host, link: &str, target: &str) -> bool {
    let Some(actual) = link_target(host, link) else {
        return false;
    };
    match host {
        Host::Native => fs::canonicalize(actual)
            .ok()
            .zip(fs::canonicalize(target).ok())
            .is_some_and(|(actual, expected)| actual == expected),
        Host::Wsl { .. } => host.canonical(&actual) == host.canonical(target),
    }
}

/// Remove one member's link, but only if it is still the link Pingex made.
pub(crate) fn remove_managed_link(
    host: &Host,
    hub: &str,
    member: &StoredWorkspaceMember,
) -> Result<(), String> {
    let link = host.join_str(hub, &member.alias);
    if !entry_exists(host, &link) {
        return Ok(());
    }
    if !link_matches(host, &link, &member.effective_path) {
        return Err(format!(
            "Workspace alias '{}' was changed outside Pingex; it will not be removed",
            member.alias
        ));
    }
    let removed = match host {
        Host::Native => fs::remove_file(&link).map_err(|error| error.to_string()),
        Host::Wsl { .. } => host
            .command("rm", &["-f", "--", &link], None, &[], &[])
            .status()
            .map_err(|error| error.to_string())
            .and_then(|status| {
                status
                    .success()
                    .then_some(())
                    .ok_or_else(|| "rm failed inside the distribution".to_string())
            }),
    };
    removed.map_err(|error| {
        format!(
            "Could not update workspace alias '{}': {error}",
            member.alias
        )
    })
}

/// Create or repair the hub for `workspace`. Idempotent: an existing correct
/// link is left alone, and a conflicting entry fails rather than clobbering.
pub(crate) fn materialize_hub(
    host: &Host,
    workspace: &StoredWorkspace,
    members: &[StoredWorkspaceMember],
) -> Result<(), String> {
    let hub = workspace.hub_path.as_str();
    fs::create_dir_all(host.to_local(hub))
        .map_err(|error| format!("Could not create workspace directory: {error}"))?;
    let metadata = host.join_str(hub, METADATA_DIR);
    fs::create_dir_all(host.to_local(&metadata))
        .map_err(|error| format!("Could not create workspace metadata directory: {error}"))?;
    for member in members {
        let link = host.join_str(hub, &member.alias);
        let target = member.effective_path.as_str();
        if entry_exists(host, &link) {
            if !link_matches(host, &link, target) {
                return Err(format!(
                    "Workspace alias '{}' already exists and is not the managed project link",
                    member.alias
                ));
            }
            continue;
        }
        link_directory(host, target, &link).map_err(|error| {
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
        host.to_local(&host.join_str(&metadata, "manifest.json")),
        serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("Could not write workspace manifest: {error}"))
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
        materialize_hub(&Host::Native, &workspace, &members).unwrap();
        let note = Path::new(&workspace.hub_path).join("NOTES.md");
        fs::write(&note, "keep this").unwrap();
        materialize_hub(&Host::Native, &workspace, &members).unwrap();
        assert_eq!(fs::read_to_string(note).unwrap(), "keep this");
        assert!(link_matches(
            &Host::Native,
            &Path::new(&workspace.hub_path)
                .join("one")
                .display()
                .to_string(),
            &source_one.display().to_string()
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
        assert!(materialize_hub(&Host::Native, &workspace, &members).is_err());
        // The user's file survives untouched.
        assert_eq!(
            fs::read_to_string(hub.join("api")).unwrap(),
            "a real file the user made"
        );
        // And it is refused for removal too.
        assert!(
            remove_managed_link(&Host::Native, &hub.display().to_string(), &members[0]).is_err()
        );
    }

    #[test]
    fn removing_an_absent_link_is_not_an_error() {
        let temp = tempfile::tempdir().unwrap();
        let hub = temp.path().join("hub");
        fs::create_dir_all(&hub).unwrap();
        assert!(remove_managed_link(
            &Host::Native,
            &hub.display().to_string(),
            &member("w1", "gone", &temp.path().join("x"))
        )
        .is_ok());
    }
}
