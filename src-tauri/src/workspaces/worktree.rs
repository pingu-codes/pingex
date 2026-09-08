//! Isolated member worktrees.
//!
//! A member marked "isolated" gets its own Git worktree on a generated branch,
//! so work in the workspace never touches the user's checked-out state in the
//! source repository. Creation is reversible: a failure part-way through a
//! multi-member workspace rolls back every worktree made so far.
//!
//! Every path here is a host path; git runs on the workspace's host.

use std::fs;
use std::path::Path;

use crate::git::run::{run_git, READ_TIMEOUT, WRITE_TIMEOUT};
use crate::util::host::Host;

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

pub(crate) fn is_git_repository(host: &Host, path: &str) -> bool {
    run_git(
        host,
        Path::new(path),
        &["rev-parse", "--is-inside-work-tree"],
        READ_TIMEOUT,
    )
    .is_ok_and(|output| output.ok)
}

fn branch_exists(host: &Host, path: &str, branch: &str) -> bool {
    run_git(
        host,
        Path::new(path),
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
        READ_TIMEOUT,
    )
    .is_ok_and(|output| output.ok)
}

/// A branch name for this member that does not already exist, suffixing a
/// counter if the natural name is taken.
pub(crate) fn available_branch(host: &Host, path: &str, workspace_id: &str, alias: &str) -> String {
    let base = format!(
        "codex/workspace-{}/{}",
        workspace_id
            .trim_start_matches("workspace-")
            .chars()
            .take(ID_CHARS)
            .collect::<String>(),
        branch_component(alias)
    );
    if !branch_exists(host, path, &base) {
        return base;
    }
    (2..)
        .map(|index| format!("{base}-{index}"))
        .find(|candidate| !branch_exists(host, path, candidate))
        .expect("unbounded iterator always finds a branch name")
}

pub(crate) fn create_isolated_worktree(
    host: &Host,
    source: &str,
    destination: &str,
    branch: &str,
) -> Result<(), String> {
    if let Some(parent) = host.parent_str(destination) {
        fs::create_dir_all(host.to_local(&parent))
            .map_err(|error| format!("Could not create workspace worktree directory: {error}"))?;
    }
    let output = run_git(
        host,
        Path::new(source),
        &["worktree", "add", "-b", branch, destination, "HEAD"],
        WRITE_TIMEOUT,
    )?;
    if output.ok {
        Ok(())
    } else {
        Err("Could not create an isolated worktree for this project".into())
    }
}

/// Undo `create_isolated_worktree`. Best-effort: this runs on a failure path
/// where there is nothing useful to report a second error to.
pub(crate) fn remove_created_worktree(host: &Host, source: &str, destination: &str, branch: &str) {
    let _ = run_git(
        host,
        Path::new(source),
        &["worktree", "remove", "--force", destination],
        WRITE_TIMEOUT,
    );
    let _ = run_git(
        host,
        Path::new(source),
        &["branch", "-D", branch],
        WRITE_TIMEOUT,
    );
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
