//! The production GitHub provider: the `gh` CLI.
//!
//! `run` spawns `gh` (argument arrays, bounded timeouts, redacted errors).
//! `parse`, `inbox`, `stacks`, `checks` and `repos` translate `gh` JSON into
//! the provider-neutral types and are tested against recorded JSON; `reads`
//! and `writes` hold the invocations. Only `GhCli` is used from outside.

mod checks;
pub(crate) mod inbox;
mod parse;
mod reads;
mod repos;
mod run;
mod stacks;
mod writes;

use std::path::PathBuf;

use super::provider::GitHubProvider;
use super::types::{
    CheckLog, CommentRef, GitHubRepo, InboxRead, MergeRequest, NewPr, PrDetail, PrHead, PrSummary,
    ProviderStatus, ReviewRequests, ReviewSubmission, StackListing,
};
use crate::util::host::Host;

/// `gh`, run on `host` inside the repository's clone at `dir`.
pub(crate) struct GhCli {
    host: Host,
    dir: PathBuf,
}

impl GhCli {
    pub(crate) fn new(host: Host, dir: PathBuf) -> Self {
        Self { host, dir }
    }
}

impl GitHubProvider for GhCli {
    fn status(&self) -> ProviderStatus {
        run::provider_status(&self.host, &self.dir)
    }

    fn repo(&self) -> Result<String, String> {
        let (owner, name) = inbox::repo_slug(&self.host, &self.dir)?;
        Ok(format!("{owner}/{name}"))
    }

    fn my_repositories(&self) -> Result<Vec<GitHubRepo>, String> {
        repos::my_repositories(&self.host, &self.dir)
    }

    fn repositories_requesting_review(&self) -> Result<Vec<ReviewRequests>, String> {
        repos::repositories_requesting_review(&self.host, &self.dir)
    }

    fn list_open_prs(&self) -> Result<Vec<PrSummary>, String> {
        reads::list_open_prs(&self.host, &self.dir)
    }

    fn inbox(&self) -> Result<InboxRead, String> {
        inbox::fetch_inbox(&self.host, &self.dir)
    }

    fn pr_detail(&self, number: i64) -> Result<PrDetail, String> {
        reads::fetch_pr_detail(&self.host, &self.dir, number)
    }

    fn pr_head(&self, number: i64) -> Result<PrHead, String> {
        reads::pr_head(&self.host, &self.dir, number)
    }

    fn stacks(&self) -> Result<StackListing, String> {
        stacks::list_stacks(&self.host, &self.dir, &self.repo()?)
    }

    fn failed_check_logs(&self, number: i64) -> Result<Vec<CheckLog>, String> {
        checks::failed_check_logs(&self.host, &self.dir, &self.repo()?, number)
    }

    fn submit_review(&self, number: i64, review: &ReviewSubmission) -> Result<(), String> {
        writes::submit_review(&self.host, &self.dir, &self.repo()?, number, review)
    }

    fn reply(&self, number: i64, comment_id: i64, body: &str) -> Result<(), String> {
        writes::reply_to_comment(
            &self.host,
            &self.dir,
            &self.repo()?,
            number,
            comment_id,
            body,
        )
    }

    fn resolve_thread(&self, thread_id: &str) -> Result<(), String> {
        writes::resolve_thread(&self.host, &self.dir, thread_id)
    }

    fn edit_comment(&self, comment: CommentRef, body: &str) -> Result<(), String> {
        writes::edit_comment(&self.host, &self.dir, &self.repo()?, comment, body)
    }

    fn delete_comment(&self, comment: CommentRef) -> Result<(), String> {
        writes::delete_comment(&self.host, &self.dir, &self.repo()?, comment)
    }

    fn merge(&self, number: i64, request: &MergeRequest) -> Result<(), String> {
        writes::merge(&self.host, &self.dir, &self.repo()?, number, request)
    }

    fn close(&self, number: i64) -> Result<(), String> {
        writes::close(&self.host, &self.dir, &self.repo()?, number)
    }

    fn retarget(&self, number: i64, base: &str) -> Result<(), String> {
        writes::retarget(&self.host, &self.dir, &self.repo()?, number, base)
    }

    fn mark_ready(&self, number: i64) -> Result<(), String> {
        writes::mark_ready(&self.host, &self.dir, &self.repo()?, number)
    }

    fn rerun_failed_checks(&self, number: i64) -> Result<(), String> {
        checks::rerun_failed_checks(&self.host, &self.dir, &self.repo()?, number)
    }

    fn create_pr(&self, pr: &NewPr) -> Result<i64, String> {
        writes::create_pr(&self.host, &self.dir, &self.repo()?, pr)
    }
}
