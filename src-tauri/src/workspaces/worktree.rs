//! Isolated member worktrees.
//!
//! A member marked "isolated" gets its own Git worktree on a generated branch,
//! so work in the workspace never touches the user's checked-out state in the
//! source repository. Creation is reversible: a failure part-way through a
//! multi-member workspace rolls back every worktree made so far.

use std::fs;
use std::path::Path;
use std::process::Command;

/// Cap on a branch component so generated names stay readable.
const MAX_COMPONENT_CHARS: usize = 40;
/// How much of the workspace id goes into the branch name.
const ID_CHARS: usize = 10;

/// Reduce a member alias to something safe inside a Git ref name.
pub(crate) fn branch_component(value: &str) -> String {
    let clean: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let clean = clean
        .trim_matches('-')
        .chars()
        .take(MAX_COMPONENT_CHARS)
        .collect::<String>();
    if clean.is_empty() {
        "member".into()
    } else {
        clean
    }
}

pub(crate) fn is_git_repository(path: &Path) -> bool {
    Command::new("git")
        .args(["-C"])
        .arg(path)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .is_ok_and(|output| output.status.success())
}

fn branch_exists(path: &Path, branch: &str) -> bool {
    Command::new("git")
        .args(["-C"])
        .arg(path)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .status()
        .is_ok_and(|status| status.success())
}

/// A branch name for this member that does not already exist, suffixing a
/// counter if the natural name is taken.
pub(crate) fn available_branch(path: &Path, workspace_id: &str, alias: &str) -> String {
    let base = format!(
        "codex/workspace-{}/{}",
        workspace_id
            .trim_start_matches("workspace-")
            .chars()
            .take(ID_CHARS)
            .collect::<String>(),
        branch_component(alias)
    );
    if !branch_exists(path, &base) {
        return base;
    }
    (2..)
        .map(|index| format!("{base}-{index}"))
        .find(|candidate| !branch_exists(path, candidate))
        .expect("unbounded iterator always finds a branch name")
}

pub(crate) fn create_isolated_worktree(
    source: &Path,
    destination: &Path,
    branch: &str,
) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create workspace worktree directory: {error}"))?;
    }
    let output = Command::new("git")
        .args(["-C"])
        .arg(source)
        .args(["worktree", "add", "-b", branch])
        .arg(destination)
        .arg("HEAD")
        .output()
        .map_err(|error| format!("Could not start git: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err("Could not create an isolated worktree for this project".into())
    }
}

/// Undo `create_isolated_worktree`. Best-effort: this runs on a failure path
/// where there is nothing useful to report a second error to.
pub(crate) fn remove_created_worktree(source: &Path, destination: &Path, branch: &str) {
    let _ = Command::new("git")
        .args(["-C"])
        .arg(source)
        .args(["worktree", "remove", "--force"])
        .arg(destination)
        .status();
    let _ = Command::new("git")
        .args(["-C"])
        .arg(source)
        .args(["branch", "-D", branch])
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_components_are_ref_safe_with_a_fallback() {
        assert_eq!(branch_component("api"), "api");
        assert_eq!(branch_component("my api/v2"), "my-api-v2");
        assert_eq!(branch_component("!!!"), "member");
        assert_eq!(branch_component("--edges--"), "edges");
        assert_eq!(branch_component(&"x".repeat(60)).len(), MAX_COMPONENT_CHARS);
    }
}
