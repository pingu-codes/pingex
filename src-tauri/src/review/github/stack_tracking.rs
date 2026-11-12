//! Making `gh stack` (the github/gh-stack extension) track a native stack in
//! its worktree, so `gh stack view`, `up`/`down`, `rebase` and `push` work
//! there as they do for a stack made with `gh stack` itself.
//!
//! `gh stack` keeps its own record of stacks in the git common directory (one
//! file shared by every worktree, in a versioned format of its own), so this
//! shells out to it and never writes that file. Observed with gh-stack 0.3.0
//! (Nov 2026), stdin closed, in a linked worktree on one of the stack's
//! branches:
//!
//! - `gh stack checkout <PR>` imports the stack from GitHub, makes any missing
//!   branch, and stays on the current branch when that PR's branch is the
//!   current one. It keeps a local branch that is ahead of GitHub's. Run again,
//!   it changes nothing.
//! - Once a stack is tracked, `checkout` doesn't re-read GitHub, so a stack
//!   whose layers changed stays stale. `gh stack unstack --local <number>`
//!   drops only the local record (without `--local` it unstacks on GitHub, so
//!   the flag is never left out), and the next `checkout` imports it afresh.
//! - `gh stack sync` would refresh it too, but it rebases and pushes.
//!
//! Nothing here reaches GitHub beyond the reads `checkout` makes.

use std::sync::LazyLock;
use std::time::Duration;

use serde_json::Value;

use super::super::provider::LocalStack;
use super::run::{redact_gh_error, run_gh, Gh, READ_TIMEOUT};
use crate::util::host::Host;
use crate::util::ttl_cache::TtlCache;

/// `checkout` fetches every branch of the stack.
const CHECKOUT_TIMEOUT: Duration = Duration::from_secs(90);

/// Whether the extension is installed, by Host, re-checked every ten minutes
/// so installing it takes effect without a restart.
static INSTALLED: LazyLock<TtlCache<Host, bool>> =
    LazyLock::new(|| TtlCache::new(Duration::from_secs(10 * 60), 16));

/// Make `gh stack` track `request`'s stack in its worktree. `Ok(false)` when
/// the extension isn't installed.
pub(crate) fn track_stack(gh: &Gh<'_>, request: &LocalStack<'_>) -> Result<bool, String> {
    if !installed(gh)? {
        return Ok(false);
    }
    let gh = Gh {
        dir: request.worktree,
        ..*gh
    };
    let tracked = tracked_branches(&gh)?;
    if tracked.as_deref() == Some(request.branches) {
        return Ok(true);
    }
    if tracked.is_some() {
        // A stale copy: drop it so `checkout` imports the stack afresh. Exit 2
        // means no stack by that number was tracked, which is fine.
        let number = request.number.to_string();
        let output = run_gh(&gh, &["stack", "unstack", "--local", &number], READ_TIMEOUT)?;
        if !output.ok && output.code != Some(2) {
            return Err(redact_gh_error(
                "gh stack could not drop its old copy of the stack",
                &output,
            ));
        }
    }
    let pr = request.current_pr.to_string();
    let output = run_gh(&gh, &["stack", "checkout", &pr], CHECKOUT_TIMEOUT)?;
    if !output.ok {
        return Err(redact_gh_error(
            "gh stack could not import the stack",
            &output,
        ));
    }
    match tracked_branches(&gh)? {
        Some(branches) if branches == request.branches => Ok(true),
        _ => {
            Err("gh stack tracks a different set of branches here than GitHub's stack".to_string())
        }
    }
}

fn installed(gh: &Gh<'_>) -> Result<bool, String> {
    INSTALLED.get_or_try(gh.host.clone(), || {
        Ok::<_, String>(run_gh(gh, &["stack", "--version"], READ_TIMEOUT)?.ok)
    })
}

/// The unmerged branches of the stack `gh stack` has active in `gh.dir`,
/// bottom first; `None` when it has none there.
fn tracked_branches(gh: &Gh<'_>) -> Result<Option<Vec<String>>, String> {
    let output = run_gh(gh, &["stack", "view", "--json"], READ_TIMEOUT)?;
    if !output.ok {
        return Ok(None);
    }
    Ok(parse_view(&output.stdout))
}

/// `gh stack view --json`'s unmerged branch names, bottom first. Outside a
/// stack it prints a message instead of JSON.
fn parse_view(stdout: &str) -> Option<Vec<String>> {
    let json: Value = serde_json::from_str(stdout).ok()?;
    let branches = json.get("branches")?.as_array()?;
    Some(
        branches
            .iter()
            .filter(|branch| {
                !branch
                    .get("isMerged")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
            .filter_map(|branch| branch.get("name").and_then(Value::as_str))
            .map(str::to_string)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_lists_the_unmerged_branches_bottom_first() {
        let stdout = r#"{
          "trunk": "master",
          "currentBranch": "b",
          "branches": [
            {"name": "a", "isMerged": true, "pr": {"number": 1}},
            {"name": "b", "isMerged": false, "pr": {"number": 2}},
            {"name": "c", "isMerged": false, "pr": {"number": 3}}
          ]
        }"#;
        assert_eq!(
            parse_view(stdout),
            Some(vec!["b".to_string(), "c".to_string()])
        );
    }

    #[test]
    fn outside_a_stack_view_tracks_nothing() {
        assert_eq!(
            parse_view("✗ current branch \"b\" is not part of a stack"),
            None
        );
    }
}
