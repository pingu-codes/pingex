//! An in-memory GitHub for Rust tests: it answers reads from what the test
//! put in and records every write it was sent.
//!
//! Tests assert on `sent()`, what GitHub received, never on call order inside
//! the desk. Reads the test did not set up answer with an error, the way a
//! PR GitHub does not have would.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::provider::GitHubProvider;
use super::types::{
    CheckLog, CommentRef, GitHubAccount, GitHubRepo, InboxPr, InboxRead, MergeRequest, NewPr,
    PrDetail, PrHead, PrSummary, ProviderStatus, ReviewRequests, ReviewSubmission, StackListing,
};

/// One write the fake received.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Sent {
    Review {
        number: i64,
        review: ReviewSubmission,
    },
    Reply {
        number: i64,
        comment_id: i64,
        body: String,
    },
    ResolveThread(String),
    EditComment {
        comment: CommentRef,
        body: String,
    },
    DeleteComment(CommentRef),
    Merge {
        number: i64,
        request: MergeRequest,
    },
    Close(i64),
    Retarget {
        number: i64,
        base: String,
    },
    MarkReady(i64),
    RerunFailedChecks(i64),
    CreatePr(NewPr),
    RenameBranch {
        from: String,
        to: String,
    },
}

#[derive(Default)]
struct State {
    viewer: String,
    prs: Vec<InboxPr>,
    details: HashMap<i64, PrDetail>,
    stacks: StackListing,
    check_logs: HashMap<i64, Vec<CheckLog>>,
    repositories: Vec<GitHubRepo>,
    review_requests: Vec<ReviewRequests>,
    sent: Vec<Sent>,
    next_pr: i64,
    /// The bare remote GitHub's git branches live in, once
    /// `merge_on_push` turned the rule on.
    remote: Option<PathBuf>,
    merged_by_push: Vec<i64>,
}

/// GitHub for one repository, `slug`.
pub(crate) struct FakeGitHub {
    slug: String,
    state: Mutex<State>,
}

impl FakeGitHub {
    pub(crate) fn new(slug: &str, viewer: &str) -> Self {
        Self {
            slug: slug.to_string(),
            state: Mutex::new(State {
                viewer: viewer.to_string(),
                next_pr: 1000,
                ..State::default()
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("fake GitHub lock")
    }

    /// Open (or update, by number) a PR in the inbox.
    pub(crate) fn put_pr(&self, pr: InboxPr) {
        let mut state = self.state();
        state.prs.retain(|existing| existing.number != pr.number);
        state.prs.push(pr);
    }

    /// Close a PR: it leaves the inbox.
    pub(crate) fn close_pr(&self, number: i64) {
        self.state().prs.retain(|pr| pr.number != number);
    }

    pub(crate) fn put_detail(&self, number: i64, detail: PrDetail) {
        self.state().details.insert(number, detail);
    }

    pub(crate) fn put_stacks(&self, stacks: StackListing) {
        self.state().stacks = stacks;
    }

    pub(crate) fn put_check_logs(&self, number: i64, logs: Vec<CheckLog>) {
        self.state().check_logs.insert(number, logs);
    }

    pub(crate) fn put_repositories(&self, repositories: Vec<GitHubRepo>) {
        self.state().repositories = repositories;
    }

    pub(crate) fn put_review_requests(&self, requests: Vec<ReviewRequests>) {
        self.state().review_requests = requests;
    }

    /// Every write received, oldest first.
    pub(crate) fn sent(&self) -> Vec<Sent> {
        self.state().sent.clone()
    }

    /// Apply GitHub's rule that an open PR whose head is reachable from its
    /// base is merged, reading git branches from the bare `remote`: from now
    /// on, before every write and in `merged_by_push`, such a PR leaves the
    /// inbox. It is what a push landing before a retarget does.
    pub(crate) fn merge_on_push(&self, remote: &Path) {
        self.state().remote = Some(remote.to_path_buf());
    }

    /// The PRs GitHub merged because a push put their head in their base.
    pub(crate) fn merged_by_push(&self) -> Vec<i64> {
        self.settle_pushes();
        self.state().merged_by_push.clone()
    }

    fn settle_pushes(&self) {
        let mut state = self.state();
        let Some(remote) = state.remote.clone() else {
            return;
        };
        let reached: Vec<i64> = state
            .prs
            .iter()
            .filter(|pr| {
                !pr.head_ref.is_empty()
                    && !pr.base_ref.is_empty()
                    && crate::git::test_repo::git_ok(
                        &remote,
                        &[
                            "merge-base",
                            "--is-ancestor",
                            &format!("refs/heads/{}", pr.head_ref),
                            &format!("refs/heads/{}", pr.base_ref),
                        ],
                    )
            })
            .map(|pr| pr.number)
            .collect();
        state.prs.retain(|pr| !reached.contains(&pr.number));
        state.merged_by_push.extend(reached);
    }

    fn record(&self, sent: Sent) -> Result<(), String> {
        self.settle_pushes();
        self.state().sent.push(sent);
        Ok(())
    }

    fn require_pr(&self, number: i64) -> Result<(), String> {
        self.settle_pushes();
        if self.state().prs.iter().any(|pr| pr.number == number) {
            Ok(())
        } else {
            Err("That pull request could not be found".to_string())
        }
    }
}

impl GitHubProvider for FakeGitHub {
    fn status(&self) -> ProviderStatus {
        ProviderStatus {
            installed: true,
            authenticated: true,
            message: None,
        }
    }

    fn repo(&self) -> Result<String, String> {
        Ok(self.slug.clone())
    }

    fn my_repositories(&self) -> Result<Vec<GitHubRepo>, String> {
        Ok(self.state().repositories.clone())
    }

    fn repositories_requesting_review(&self) -> Result<Vec<ReviewRequests>, String> {
        Ok(self.state().review_requests.clone())
    }

    fn account(&self) -> Result<GitHubAccount, String> {
        let viewer = self.state().viewer.clone();
        Ok(GitHubAccount {
            login: Some(viewer.clone()),
            token_source: Some("keyring".to_string()),
            logins: vec![viewer],
        })
    }

    fn switch_account(&self, login: &str) -> Result<(), String> {
        self.state().viewer = login.to_string();
        Ok(())
    }

    fn list_open_prs(&self) -> Result<Vec<PrSummary>, String> {
        Ok(self
            .state()
            .prs
            .iter()
            .map(|pr| PrSummary {
                number: pr.number,
                title: pr.title.clone(),
                author: pr.author.clone(),
                state: "OPEN".to_string(),
                is_draft: pr.is_draft,
                base_ref: pr.base_ref.clone(),
                head_ref: pr.head_ref.clone(),
                updated_at: pr.updated_at.clone(),
                url: pr.url.clone(),
            })
            .collect())
    }

    fn inbox(&self) -> Result<InboxRead, String> {
        let state = self.state();
        Ok(InboxRead {
            repo: self.slug.clone(),
            viewer: state.viewer.clone(),
            prs: state.prs.clone(),
            rate_limit: None,
            complete: true,
        })
    }

    fn pr_detail(&self, number: i64) -> Result<PrDetail, String> {
        self.state()
            .details
            .get(&number)
            .cloned()
            .ok_or_else(|| "That pull request could not be found".to_string())
    }

    fn pr_head(&self, number: i64) -> Result<PrHead, String> {
        self.state()
            .prs
            .iter()
            .find(|pr| pr.number == number)
            .map(|pr| PrHead {
                head_sha: pr.head_sha.clone(),
                updated_at: pr.updated_at.clone(),
            })
            .ok_or_else(|| "That pull request could not be found".to_string())
    }

    fn stacks(&self) -> Result<StackListing, String> {
        Ok(self.state().stacks.clone())
    }

    fn failed_check_logs(&self, number: i64) -> Result<Vec<CheckLog>, String> {
        Ok(self
            .state()
            .check_logs
            .get(&number)
            .cloned()
            .unwrap_or_default())
    }

    fn submit_review(&self, number: i64, review: &ReviewSubmission) -> Result<(), String> {
        self.require_pr(number)?;
        self.record(Sent::Review {
            number,
            review: review.clone(),
        })
    }

    fn reply(&self, number: i64, comment_id: i64, body: &str) -> Result<(), String> {
        self.require_pr(number)?;
        self.record(Sent::Reply {
            number,
            comment_id,
            body: body.to_string(),
        })
    }

    fn resolve_thread(&self, thread_id: &str) -> Result<(), String> {
        self.record(Sent::ResolveThread(thread_id.to_string()))
    }

    fn edit_comment(&self, comment: CommentRef, body: &str) -> Result<(), String> {
        self.record(Sent::EditComment {
            comment,
            body: body.to_string(),
        })
    }

    fn delete_comment(&self, comment: CommentRef) -> Result<(), String> {
        self.record(Sent::DeleteComment(comment))
    }

    fn merge(&self, number: i64, request: &MergeRequest) -> Result<(), String> {
        self.require_pr(number)?;
        self.record(Sent::Merge {
            number,
            request: request.clone(),
        })?;
        self.close_pr(number);
        Ok(())
    }

    fn close(&self, number: i64) -> Result<(), String> {
        self.require_pr(number)?;
        self.record(Sent::Close(number))?;
        self.close_pr(number);
        Ok(())
    }

    fn retarget(&self, number: i64, base: &str) -> Result<(), String> {
        self.require_pr(number)?;
        self.record(Sent::Retarget {
            number,
            base: base.to_string(),
        })?;
        if let Some(pr) = self.state().prs.iter_mut().find(|pr| pr.number == number) {
            pr.base_ref = base.to_string();
        }
        Ok(())
    }

    fn mark_ready(&self, number: i64) -> Result<(), String> {
        self.require_pr(number)?;
        self.record(Sent::MarkReady(number))?;
        if let Some(pr) = self.state().prs.iter_mut().find(|pr| pr.number == number) {
            pr.is_draft = false;
        }
        Ok(())
    }

    fn rerun_failed_checks(&self, number: i64) -> Result<(), String> {
        self.require_pr(number)?;
        self.record(Sent::RerunFailedChecks(number))
    }

    fn create_pr(&self, pr: &NewPr) -> Result<i64, String> {
        self.record(Sent::CreatePr(pr.clone()))?;
        let mut state = self.state();
        let number = state.next_pr;
        state.next_pr += 1;
        let viewer = state.viewer.clone();
        state.prs.push(InboxPr {
            number,
            title: pr.title.clone(),
            author: viewer,
            is_draft: pr.draft,
            head_ref: pr.head.clone(),
            base_ref: pr.base.clone(),
            ..InboxPr::default()
        });
        Ok(number)
    }

    fn rename_branch(&self, from: &str, to: &str) -> Result<(), String> {
        self.record(Sent::RenameBranch {
            from: from.to_string(),
            to: to.to_string(),
        })?;
        for pr in self.state().prs.iter_mut() {
            if pr.head_ref == from {
                pr.head_ref = to.to_string();
            }
            if pr.base_ref == from {
                pr.base_ref = to.to_string();
            }
        }
        Ok(())
    }
}
