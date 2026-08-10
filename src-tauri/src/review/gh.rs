//! Running the GitHub CLI.
//!
//! Every invocation uses an argument array (never a shell string), a bounded
//! timeout, and redacted errors so raw stderr never leaks. `gh` is denied stdin
//! and any pager or editor, so a command that would prompt fails fast instead of
//! hanging the review view.

use std::path::Path;
use std::time::Duration;

use super::types::ProviderStatus;
use crate::util::process::{self, CommandOutput, Run, RunError};

/// `gh` read commands should return quickly; a slow network is reported as a
/// timeout rather than hanging the review view.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(20);
/// Mutations (submitting a review, replying, resolving) can take a little longer.
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Environment that keeps `gh` non-interactive and machine-readable.
const NON_INTERACTIVE: [(&str, &str); 3] = [
    ("GH_PAGER", ""),
    ("GH_PROMPT_DISABLED", "1"),
    ("NO_COLOR", "1"),
];

fn describe(error: RunError) -> String {
    match error {
        RunError::NotFound => "GitHub CLI (gh) is not installed or not on PATH".to_string(),
        RunError::Spawn => "Could not start gh".to_string(),
        RunError::Timeout => "gh timed out".to_string(),
        RunError::NoOutput => "gh did not produce any output".to_string(),
    }
}

/// Run `gh <args...>` inside `dir` with a timeout. A missing executable or a
/// timeout is returned as a redacted error; a non-zero exit is surfaced through
/// `CommandOutput::ok` so callers classify it (e.g. "not authenticated").
pub(crate) fn run_gh(
    dir: &Path,
    args: &[&str],
    timeout: Duration,
) -> Result<CommandOutput, String> {
    process::run(Run {
        program: "gh",
        dir,
        args,
        env: &NON_INTERACTIVE,
        stdin: None,
        timeout,
    })
    .map_err(describe)
}

/// POST a JSON body to a `gh api` endpoint by piping it through stdin. Used for
/// payloads (a review with nested inline comments) that `-f` fields cannot express.
pub(crate) fn run_gh_with_input(
    dir: &Path,
    endpoint: &str,
    body: &str,
) -> Result<CommandOutput, String> {
    process::run(Run {
        program: "gh",
        dir,
        args: &["api", "--method", "POST", endpoint, "--input", "-"],
        env: &NON_INTERACTIVE,
        stdin: Some(body),
        timeout: WRITE_TIMEOUT,
    })
    .map_err(describe)
}

/// Redact a `gh` failure, promoting a couple of well-known messages to an
/// actionable form and otherwise using a generic fallback.
pub(crate) fn redact_gh_error(fallback: &str, output: &CommandOutput) -> String {
    let stderr = output.stderr.to_lowercase();
    if stderr.contains("could not resolve to a repository")
        || stderr.contains("no git remotes found")
        || stderr.contains("none of the git remotes")
    {
        "This folder has no GitHub repository (no matching remote)".to_string()
    } else if stderr.contains("authentication") || stderr.contains("gh auth login") {
        "GitHub CLI is not authenticated — run `gh auth login`".to_string()
    } else if stderr.contains("not found") {
        "That pull request could not be found".to_string()
    } else {
        fallback.to_string()
    }
}

/// Probe whether `gh` is installed and logged in.
pub(crate) fn provider_status(dir: &Path) -> ProviderStatus {
    let version = run_gh(dir, &["--version"], Duration::from_secs(5));
    let installed = matches!(version, Ok(ref output) if output.ok);
    if !installed {
        return ProviderStatus {
            installed: false,
            authenticated: false,
            message: Some("Install the GitHub CLI (gh) to review pull requests".to_string()),
        };
    }
    match run_gh(dir, &["auth", "status"], Duration::from_secs(10)) {
        Ok(output) if output.ok => ProviderStatus {
            installed: true,
            authenticated: true,
            message: None,
        },
        _ => ProviderStatus {
            installed: true,
            authenticated: false,
            message: Some("Sign in with `gh auth login` to review pull requests".to_string()),
        },
    }
}

/// Resolve `owner/name` for the repository in `dir` via `gh repo view`.
pub(crate) fn repo_name_with_owner(dir: &Path) -> Result<(String, String), String> {
    let output = run_gh(
        dir,
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ],
        READ_TIMEOUT,
    )?;
    if !output.ok {
        return Err(redact_gh_error(
            "Could not resolve the GitHub repository",
            &output,
        ));
    }
    let slug = output.stdout.trim();
    let (owner, name) = slug
        .split_once('/')
        .ok_or_else(|| "Could not resolve the GitHub repository".to_string())?;
    Ok((owner.to_string(), name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stderr: &str) -> CommandOutput {
        CommandOutput {
            ok: false,
            stdout: String::new(),
            stderr: stderr.to_string(),
        }
    }

    #[test]
    fn known_gh_failures_become_actionable_messages() {
        assert_eq!(
            redact_gh_error("fallback", &output("Could not resolve to a Repository")),
            "This folder has no GitHub repository (no matching remote)"
        );
        assert_eq!(
            redact_gh_error("fallback", &output("run gh auth login to authenticate")),
            "GitHub CLI is not authenticated — run `gh auth login`"
        );
        assert_eq!(
            redact_gh_error("fallback", &output("HTTP 404: Not Found")),
            "That pull request could not be found"
        );
    }

    #[test]
    fn an_unrecognised_failure_never_leaks_stderr() {
        let redacted = redact_gh_error("Could not submit the review", &output("token ghp_secret"));
        assert_eq!(redacted, "Could not submit the review");
        assert!(!redacted.contains("ghp_secret"));
    }
}
