//! A PR's failing checks: their logs, and re-running them.
//!
//! `gh pr checks --json` lists every check at the head with a link. A GitHub
//! Actions job links to `.../actions/runs/<run>/job/<job>`; checks from other
//! CI services have no run and job to address and are skipped.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

use crate::review::types::CheckLog;
use crate::util::host::Host;

use super::run::{redact_gh_error, run_gh, run_gh_capped, READ_TIMEOUT, WRITE_TIMEOUT};

/// One log is plenty for an agent's prompt or the log view.
const MAX_LOG_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FailedJob {
    pub(crate) name: String,
    pub(crate) run_id: u64,
    pub(crate) job_id: u64,
}

/// `.../actions/runs/<run>/job/<job>` as `(run, job)`.
fn run_and_job(link: &str) -> Option<(u64, u64)> {
    let rest = link.split("/actions/runs/").nth(1)?;
    let mut parts = rest.split('/');
    let run = parts.next()?.parse().ok()?;
    (parts.next()? == "job").then_some(())?;
    let job = parts.next()?.split(['?', '#']).next()?.parse().ok()?;
    Some((run, job))
}

/// The failing GitHub Actions jobs in `gh pr checks --json name,bucket,link`.
pub(crate) fn parse_failed_jobs(json: &Value) -> Vec<FailedJob> {
    json.as_array()
        .into_iter()
        .flatten()
        .filter(|check| check.get("bucket").and_then(Value::as_str) == Some("fail"))
        .filter_map(|check| {
            let link = check.get("link").and_then(Value::as_str)?;
            let (run_id, job_id) = run_and_job(link)?;
            Some(FailedJob {
                name: check
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                run_id,
                job_id,
            })
        })
        .collect()
}

fn failed_jobs(host: &Host, dir: &Path, slug: &str, number: i64) -> Result<Vec<FailedJob>, String> {
    let number = number.to_string();
    let output = run_gh(
        host,
        dir,
        &[
            "pr",
            "checks",
            &number,
            "--repo",
            slug,
            "--json",
            "name,bucket,link",
        ],
        READ_TIMEOUT,
    )?;
    // `gh pr checks` exits non-zero when a check is failing or pending, which
    // is exactly when it is asked; the JSON is still on stdout.
    match serde_json::from_str::<Value>(&output.stdout) {
        Ok(json) => Ok(parse_failed_jobs(&json)),
        Err(_) if !output.ok => Err(redact_gh_error("Could not read the checks", &output)),
        Err(_) => Err("Could not parse the checks".to_string()),
    }
}

pub(crate) fn failed_check_logs(
    host: &Host,
    dir: &Path,
    slug: &str,
    number: i64,
) -> Result<Vec<CheckLog>, String> {
    failed_jobs(host, dir, slug, number)?
        .into_iter()
        .map(|job| {
            let job_id = job.job_id.to_string();
            let output = run_gh_capped(
                host,
                dir,
                &[
                    "run",
                    "view",
                    "--job",
                    &job_id,
                    "--log-failed",
                    "--repo",
                    slug,
                ],
                READ_TIMEOUT,
                MAX_LOG_BYTES,
            )?;
            if !output.ok && !output.truncated {
                return Err(redact_gh_error("Could not read the check's log", &output));
            }
            Ok(CheckLog {
                name: job.name,
                log: output.stdout,
                truncated: output.truncated,
            })
        })
        .collect()
}

pub(crate) fn rerun_failed_checks(
    host: &Host,
    dir: &Path,
    slug: &str,
    number: i64,
) -> Result<(), String> {
    let runs: BTreeSet<u64> = failed_jobs(host, dir, slug, number)?
        .into_iter()
        .map(|job| job.run_id)
        .collect();
    for run in runs {
        let run = run.to_string();
        let output = run_gh(
            host,
            dir,
            &["run", "rerun", &run, "--failed", "--repo", slug],
            WRITE_TIMEOUT,
        )?;
        if !output.ok {
            return Err(redact_gh_error("Could not re-run the checks", &output));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_failing_actions_jobs_are_addressable() {
        let checks = json!([
            {"name": "test", "bucket": "fail",
             "link": "https://github.com/o/r/actions/runs/11/job/22?pr=4"},
            {"name": "lint", "bucket": "pass",
             "link": "https://github.com/o/r/actions/runs/11/job/23"},
            {"name": "external", "bucket": "fail", "link": "https://ci.example.com/b/9"},
            {"name": "build", "bucket": "fail",
             "link": "https://github.com/o/r/actions/runs/12/job/30"}
        ]);
        let jobs = parse_failed_jobs(&checks);
        let found: Vec<(&str, u64, u64)> = jobs
            .iter()
            .map(|job| (job.name.as_str(), job.run_id, job.job_id))
            .collect();
        assert_eq!(found, [("test", 11, 22), ("build", 12, 30)]);
        assert!(parse_failed_jobs(&json!({"message": "x"})).is_empty());
    }
}
