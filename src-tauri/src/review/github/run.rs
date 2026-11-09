//! Running the GitHub CLI.
//!
//! Every invocation uses an argument array (never a shell string), a bounded
//! timeout, and redacted errors so raw stderr never leaks. `gh` is denied stdin
//! and any pager or editor, so a command that would prompt fails fast instead of
//! hanging the review view.

use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use super::account::GhToken;
use crate::review::types::ProviderStatus;
use crate::util::host::Host;
use crate::util::process::{self, CommandOutput, Run, RunError};

/// `gh` read commands should return quickly; a slow network is reported as a
/// timeout rather than hanging the review view.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(20);
/// Mutations (submitting a review, replying, resolving) can take a little longer.
pub(crate) const WRITE_TIMEOUT: Duration = Duration::from_secs(30);

/// Environment that keeps `gh` non-interactive and machine-readable.
const NON_INTERACTIVE: [(&str, &str); 4] = [
    ("GH_PAGER", ""),
    ("GH_PROMPT_DISABLED", "1"),
    ("NO_COLOR", "1"),
    // gh asks to upgrade itself on stderr; never in the middle of a read.
    ("GH_NO_UPDATE_NOTIFIER", "1"),
];

/// Where and as whom one `gh` command runs. `token` is `None` when gh's own
/// active account is the right one.
#[derive(Clone, Copy)]
pub(crate) struct Gh<'a> {
    pub(crate) host: &'a Host,
    pub(crate) dir: &'a Path,
    /// The login the command acts as; empty for the account-level commands,
    /// which answer for whichever account is active.
    pub(crate) login: &'a str,
    pub(crate) token: Option<&'a GhToken>,
}

fn describe(error: RunError) -> String {
    match error {
        RunError::NotFound => "GitHub CLI (gh) is not installed or not on PATH".to_string(),
        RunError::Spawn => "Could not start gh".to_string(),
        RunError::Timeout => "gh timed out".to_string(),
        RunError::NoOutput => "gh did not produce any output".to_string(),
    }
}

fn spawn_gh(
    gh: &Gh<'_>,
    args: &[&str],
    stdin: Option<&str>,
    timeout: Duration,
    max_stdout: Option<usize>,
) -> Result<CommandOutput, String> {
    let dir = gh.host.path_string(gh.dir);
    let binary = gh.host.resolve_binary("gh").unwrap_or_else(|| "gh".into());
    process::run(Run {
        host: gh.host,
        program: &binary,
        dir: &dir,
        args,
        env: &NON_INTERACTIVE,
        secret: gh.token.map(|token| ("GH_TOKEN", token.expose())),
        stdin,
        timeout,
        max_stdout,
    })
    .map_err(describe)
}

/// Run `gh <args...>` on `host` inside `dir` (a host path) with a timeout. A
/// missing executable or a timeout is returned as a redacted error; a non-zero
/// exit is surfaced through `CommandOutput::ok` so callers classify it (e.g.
/// "not authenticated"). The host comes from the window's Home, like git's, so
/// a WSL Home runs the distribution's own `gh` and login.
pub(crate) fn run_gh(
    gh: &Gh<'_>,
    args: &[&str],
    timeout: Duration,
) -> Result<CommandOutput, String> {
    spawn_gh(gh, args, None, timeout, None)
}

/// `run_gh`, reading at most `max_stdout` bytes (`CommandOutput::truncated`
/// says when it stopped), for output that can be huge, like a CI log.
pub(crate) fn run_gh_capped(
    gh: &Gh<'_>,
    args: &[&str],
    timeout: Duration,
    max_stdout: usize,
) -> Result<CommandOutput, String> {
    spawn_gh(gh, args, None, timeout, Some(max_stdout))
}

/// POST a JSON body to a `gh api` endpoint by piping it through stdin. Used for
/// payloads (a review with nested inline comments) that `-f` fields cannot express.
pub(crate) fn run_gh_with_input(
    gh: &Gh<'_>,
    endpoint: &str,
    body: &str,
) -> Result<CommandOutput, String> {
    spawn_gh(
        gh,
        &["api", "--method", "POST", endpoint, "--input", "-"],
        Some(body),
        WRITE_TIMEOUT,
        None,
    )
}

/// A GraphQL variable. Strings go as `-f` (raw: a value starting with `@` is
/// never read as a file); integers as `-F`, which gh sends typed.
pub(crate) enum GqlVar<'a> {
    Str(&'a str),
    Int(i64),
}

/// The pages of one GraphQL read.
#[derive(Debug, Default)]
pub(crate) struct GqlPages {
    pub(crate) pages: Vec<Value>,
    /// False when `max_pages` ran out before the connection did.
    pub(crate) complete: bool,
}

/// The first `pageInfo` in a response, depth first: the one connection a
/// paginated query follows.
fn page_info(value: &Value) -> Option<(bool, Option<String>)> {
    match value {
        Value::Object(map) => {
            if let Some(info) = map.get("pageInfo") {
                let more = info
                    .get("hasNextPage")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let cursor = info
                    .get("endCursor")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Some((more, cursor));
            }
            map.values().find_map(page_info)
        }
        Value::Array(items) => items.iter().find_map(page_info),
        _ => None,
    }
}

/// Run a GraphQL read through `gh api graphql`, one request per page, up to
/// `max_pages`.
///
/// A query that pages takes `$endCursor: String` and selects
/// `pageInfo { hasNextPage endCursor }` on its one paginated connection. The
/// pages are fetched here rather than with `gh --paginate` so the number of
/// pages (and so the time a read can take) is bounded, and each page gets its
/// own timeout. A response with GraphQL `errors` is an error even when gh
/// exits 0. Keep pages small: GitHub answers a query that takes longer than
/// about 10s with a 502.
pub(crate) fn gh_graphql(
    gh: &Gh<'_>,
    query: &str,
    vars: &[(&str, GqlVar<'_>)],
    max_pages: usize,
) -> Result<GqlPages, String> {
    let query_field = format!("query={query}");
    let var_fields: Vec<(&str, String)> = vars
        .iter()
        .map(|(name, value)| match value {
            GqlVar::Str(text) => ("-f", format!("{name}={text}")),
            GqlVar::Int(number) => ("-F", format!("{name}={number}")),
        })
        .collect();
    let mut result = GqlPages::default();
    let mut cursor: Option<String> = None;
    while result.pages.len() < max_pages.max(1) {
        let cursor_field = cursor.as_ref().map(|cursor| format!("endCursor={cursor}"));
        let mut args = vec!["api", "graphql", "-f", query_field.as_str()];
        for (flag, field) in &var_fields {
            args.extend([*flag, field.as_str()]);
        }
        if let Some(field) = &cursor_field {
            args.extend(["-f", field.as_str()]);
        }
        let output = run_gh(gh, &args, READ_TIMEOUT)?;
        if !output.ok {
            return Err(redact_gh_error("GitHub did not answer the query", &output));
        }
        let page: Value = serde_json::from_str(&output.stdout)
            .map_err(|_| "Could not parse GitHub's answer".to_string())?;
        if page.get("errors").is_some() {
            return Err("GitHub refused part of the query".to_string());
        }
        let next = page_info(&page);
        result.pages.push(page);
        match next {
            Some((true, Some(next))) => cursor = Some(next),
            _ => {
                result.complete = true;
                break;
            }
        }
    }
    Ok(result)
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
pub(crate) fn provider_status(gh: &Gh<'_>) -> ProviderStatus {
    let version = run_gh(gh, &["--version"], Duration::from_secs(5));
    let installed = matches!(version, Ok(ref output) if output.ok);
    if !installed {
        return ProviderStatus {
            installed: false,
            authenticated: false,
            message: Some("Install the GitHub CLI (gh) to review pull requests".to_string()),
        };
    }
    match run_gh(gh, &["auth", "status"], Duration::from_secs(10)) {
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
pub(crate) fn repo_name_with_owner(gh: &Gh<'_>) -> Result<(String, String), String> {
    let output = run_gh(
        gh,
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
            stderr: stderr.to_string(),
            ..CommandOutput::default()
        }
    }

    #[test]
    fn finds_the_paginated_connection() {
        let page = serde_json::json!({"data": {"repository": {"pullRequests": {
            "pageInfo": {"hasNextPage": true, "endCursor": "Y3Vyc29y"},
            "nodes": [{"reviewThreads": {"nodes": []}}]
        }}}});
        assert_eq!(page_info(&page), Some((true, Some("Y3Vyc29y".to_string()))));
        assert_eq!(page_info(&serde_json::json!({"data": {}})), None);
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
