//! The GitHub provider port: every question the review desk asks GitHub and
//! every change it makes there.
//!
//! `github::GhCli` is the production adapter, over the `gh` CLI; tests use
//! `fake::FakeGitHub`, which records what it was sent. Nothing outside the
//! adapter names `gh`. An adapter for another host (GitLab, GitHub
//! Enterprise Server) would implement this trait over its own API and map
//! onto the same provider-neutral types.
//!
//! One provider value serves one repository, as the account that repository
//! resolves to (see `RepositoryAccount`). The account-level operations
//! (`my_repositories`, `repositories_requesting_review`, `account`,
//! `switch_account`) answer for the global signed-in account whichever
//! repository the value was made for.
//!
//! Every call blocks (it is a subprocess or a network request), so the desk
//! runs them off the async runtime. Adding an operation: add it here, in
//! `GhCli`, and in `FakeGitHub` (with a `Sent` variant for a write).

use super::types::{
    CheckLog, CommentRef, GitHubAccount, GitHubRepo, InboxRead, MergeRequest, NewPr, PrDetail,
    PrHead, PrSummary, ProviderStatus, RepositoryAccount, ReviewRequests, ReviewSubmission,
    StackListing,
};

pub(crate) trait GitHubProvider: Send + Sync {
    // --- Account and repository ---

    /// Whether the provider is installed and signed in.
    fn status(&self) -> ProviderStatus;
    /// The repository this provider serves, as `owner/name`.
    fn repo(&self) -> Result<String, String>;
    /// Repositories the signed-in account owns or belongs to, most recently
    /// pushed first.
    fn my_repositories(&self) -> Result<Vec<GitHubRepo>, String>;
    /// Repositories with open PRs that ask the signed-in account for review,
    /// most requests first.
    fn repositories_requesting_review(&self) -> Result<Vec<ReviewRequests>, String>;
    /// The signed-in account, where its token comes from, and the other
    /// accounts signed in alongside it.
    fn account(&self) -> Result<GitHubAccount, String>;
    /// The account this repository's operations run as, given the login the
    /// user chose for it, if any.
    fn repository_account(&self, pin: Option<&str>) -> Result<RepositoryAccount, String>;
    /// Make `login`, already signed in, the active account. Changes only the
    /// local sign-in, nothing on GitHub.
    fn switch_account(&self, login: &str) -> Result<(), String>;

    // --- Reads ---

    /// Open PRs for the older single-PR picker.
    fn list_open_prs(&self) -> Result<Vec<PrSummary>, String>;
    /// Every open PR with its inbox facts (review requests, review decision,
    /// checks, unresolved threads, base and head), in one batched read.
    fn inbox(&self) -> Result<InboxRead, String>;
    /// One PR's detail: body, commits, files, conversation and threads.
    fn pr_detail(&self, number: i64) -> Result<PrDetail, String>;
    /// One PR's current head, cheaply, for the stale-head check.
    fn pr_head(&self, number: i64) -> Result<PrHead, String>;
    /// The repository's native stacks, or `supported: false`.
    fn stacks(&self) -> Result<StackListing, String>;
    /// The logs of the PR's failing checks at its head.
    fn failed_check_logs(&self, number: i64) -> Result<Vec<CheckLog>, String>;

    // --- Review writes ---

    /// Post one review, with its inline comments, in one call.
    fn submit_review(&self, number: i64, review: &ReviewSubmission) -> Result<(), String>;
    /// Reply in the thread of inline comment `comment_id`, at once.
    fn reply(&self, number: i64, comment_id: i64, body: &str) -> Result<(), String>;
    fn resolve_thread(&self, thread_id: &str) -> Result<(), String>;
    fn edit_comment(&self, comment: CommentRef, body: &str) -> Result<(), String>;
    fn delete_comment(&self, comment: CommentRef) -> Result<(), String>;

    // --- PR writes ---

    fn merge(&self, number: i64, request: &MergeRequest) -> Result<(), String>;
    fn close(&self, number: i64) -> Result<(), String>;
    /// Change the git branch the PR merges into.
    fn retarget(&self, number: i64, base: &str) -> Result<(), String>;
    /// Take the PR out of draft.
    fn mark_ready(&self, number: i64) -> Result<(), String>;
    /// Re-run the failed jobs of the PR's checks at its head.
    fn rerun_failed_checks(&self, number: i64) -> Result<(), String>;
    /// Open a PR and return its number.
    fn create_pr(&self, pr: &NewPr) -> Result<i64, String>;
    /// Rename a git branch on GitHub. PRs whose head or base it is follow
    /// the new name.
    fn rename_branch(&self, from: &str, to: &str) -> Result<(), String>;
}
