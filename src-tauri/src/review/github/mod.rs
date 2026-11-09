//! The production GitHub provider: the `gh` CLI.
//!
//! `run` spawns `gh` (argument arrays, bounded timeouts, redacted errors).
//! `parse`, `inbox`, `stacks`, `checks` and `repos` translate `gh` JSON into
//! the provider-neutral types and are tested against recorded JSON; `reads`
//! and `writes` hold the invocations. `account` chooses which signed-in
//! account a repository's commands run as. Only `GhCli` is used from outside.

mod account;
mod checks;
pub(crate) mod inbox;
mod parse;
mod reads;
mod repos;
mod run;
mod stacks;
mod writes;

use std::path::PathBuf;
use std::sync::OnceLock;

use turso::Database;

#[cfg(test)]
pub(crate) use self::account::describe_account;
use self::account::Scope;
use self::run::Gh;
use super::provider::GitHubProvider;
use super::types::{
    CheckLog, CommentRef, GitHubAccount, GitHubRepo, InboxRead, MergeRequest, NewPr, PrDetail,
    PrHead, PrSummary, ProviderStatus, RepositoryAccount, ReviewRequests, ReviewSubmission,
    StackListing,
};
use crate::util::host::Host;

/// `gh`, run on `host` inside the repository's clone at `dir`. A value lives
/// for one command, so it holds the repository's account, and any token for
/// it, no longer than that.
pub(crate) struct GhCli {
    host: Host,
    dir: PathBuf,
    database: Database,
    scope: OnceLock<Scope>,
}

impl GhCli {
    pub(crate) fn new(host: Host, dir: PathBuf, database: Database) -> Self {
        Self {
            host,
            dir,
            database,
            scope: OnceLock::new(),
        }
    }

    /// For the account-level commands: gh's active account.
    fn global(&self) -> Gh<'_> {
        Gh {
            host: &self.host,
            dir: &self.dir,
            login: "",
            token: None,
        }
    }

    /// For the repository's commands: the account it resolves to. A failure
    /// is returned rather than falling back to another account, so a write
    /// can't post as the wrong person.
    fn gh(&self) -> Result<Gh<'_>, String> {
        let scope = match self.scope.get() {
            Some(scope) => scope,
            None => {
                let scope = account::open_scope(&self.global(), &self.database)?;
                self.scope.get_or_init(|| scope)
            }
        };
        Ok(Gh {
            host: &self.host,
            dir: &self.dir,
            login: &scope.login,
            token: scope.token.as_ref(),
        })
    }
}

impl GitHubProvider for GhCli {
    fn status(&self) -> ProviderStatus {
        run::provider_status(&self.global())
    }

    fn repo(&self) -> Result<String, String> {
        let (owner, name) = inbox::repo_slug(&self.gh()?)?;
        Ok(format!("{owner}/{name}"))
    }

    fn my_repositories(&self) -> Result<Vec<GitHubRepo>, String> {
        repos::my_repositories(&self.global())
    }

    fn repositories_requesting_review(&self) -> Result<Vec<ReviewRequests>, String> {
        repos::repositories_requesting_review(&self.global())
    }

    fn account(&self) -> Result<GitHubAccount, String> {
        account::fresh_account(&self.global())
    }

    fn repository_account(&self, pin: Option<&str>) -> Result<RepositoryAccount, String> {
        account::repository_account(&self.global(), pin)
    }

    fn switch_account(&self, login: &str) -> Result<(), String> {
        repos::switch_account(&self.global(), login)?;
        // The next resolution should see it; a failed read expires by itself.
        let _ = account::fresh_account(&self.global());
        Ok(())
    }

    fn list_open_prs(&self) -> Result<Vec<PrSummary>, String> {
        reads::list_open_prs(&self.gh()?)
    }

    fn inbox(&self) -> Result<InboxRead, String> {
        inbox::fetch_inbox(&self.gh()?)
    }

    fn pr_detail(&self, number: i64) -> Result<PrDetail, String> {
        reads::fetch_pr_detail(&self.gh()?, number)
    }

    fn pr_head(&self, number: i64) -> Result<PrHead, String> {
        reads::pr_head(&self.gh()?, number)
    }

    fn stacks(&self) -> Result<StackListing, String> {
        stacks::list_stacks(&self.gh()?, &self.repo()?)
    }

    fn failed_check_logs(&self, number: i64) -> Result<Vec<CheckLog>, String> {
        checks::failed_check_logs(&self.gh()?, &self.repo()?, number)
    }

    fn submit_review(&self, number: i64, review: &ReviewSubmission) -> Result<(), String> {
        writes::submit_review(&self.gh()?, &self.repo()?, number, review)
    }

    fn reply(&self, number: i64, comment_id: i64, body: &str) -> Result<(), String> {
        writes::reply_to_comment(&self.gh()?, &self.repo()?, number, comment_id, body)
    }

    fn resolve_thread(&self, thread_id: &str) -> Result<(), String> {
        writes::resolve_thread(&self.gh()?, thread_id)
    }

    fn edit_comment(&self, comment: CommentRef, body: &str) -> Result<(), String> {
        writes::edit_comment(&self.gh()?, &self.repo()?, comment, body)
    }

    fn delete_comment(&self, comment: CommentRef) -> Result<(), String> {
        writes::delete_comment(&self.gh()?, &self.repo()?, comment)
    }

    fn merge(&self, number: i64, request: &MergeRequest) -> Result<(), String> {
        writes::merge(&self.gh()?, &self.repo()?, number, request)
    }

    fn close(&self, number: i64) -> Result<(), String> {
        writes::close(&self.gh()?, &self.repo()?, number)
    }

    fn retarget(&self, number: i64, base: &str) -> Result<(), String> {
        writes::retarget(&self.gh()?, &self.repo()?, number, base)
    }

    fn mark_ready(&self, number: i64) -> Result<(), String> {
        writes::mark_ready(&self.gh()?, &self.repo()?, number)
    }

    fn rerun_failed_checks(&self, number: i64) -> Result<(), String> {
        checks::rerun_failed_checks(&self.gh()?, &self.repo()?, number)
    }

    fn create_pr(&self, pr: &NewPr) -> Result<i64, String> {
        writes::create_pr(&self.gh()?, &self.repo()?, pr)
    }

    fn rename_branch(&self, from: &str, to: &str) -> Result<(), String> {
        writes::rename_branch(&self.gh()?, &self.repo()?, from, to)
    }
}
