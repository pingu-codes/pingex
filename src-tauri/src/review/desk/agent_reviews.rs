//! Agent reviews: the queue, the first message, and the Findings that come
//! back.
//!
//! Start review enqueues a run. The queue lets `MAX_RUNNING` run at once
//! across the Home; the frontend claims the runs it may start, starts each
//! as a thread filed under the PR, and hands back the final message, which
//! `findings` parses into stored drafts. Dismiss reasons and the
//! repository's review rules feed the next run's prompt.

use std::collections::HashSet;
use std::sync::LazyLock;

use tokio::sync::Mutex;
use turso::Database;

use super::{pr_number, ReviewDesk, PROVIDER};
use crate::git::objects;
use crate::harness::Capability;
use crate::review::agent_prompt::{agent_review_prompt, PromptInput, SinceLastRun, FILE_BUDGET};
use crate::review::agent_review::{
    AgentReview, AgentReviewRequest, AgentReviewStatus, FocusArea, ReviewRule, MAX_RUNNING,
};
use crate::review::checkout_types::CheckoutTarget;
use crate::review::findings::{
    findings_from_report, parse_findings_block, DismissReason, Finding, FindingState, ReviewedPr,
};
use crate::review::requirements::CheckoutStep;
use crate::review::types::PrDetail;
use crate::storage;
use crate::util::id::unique_suffix;
use crate::util::time::unix_millis;

/// Claims run one at a time, so two callers never start the same run.
static CLAIM: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

/// How many runs the repository listing returns.
const LISTED: i64 = 50;

/// Whether `request` can run as asked: a harness without a read-only sandbox
/// only reviews in a worktree, and running tests needs code on disk.
fn check_request(request: &AgentReviewRequest) -> Result<(), String> {
    if request.targets.is_empty() {
        return Err("Pick a pull request to review".to_string());
    }
    if request.step < CheckoutStep::Worktree
        && !request
            .harness
            .capabilities()
            .contains(&Capability::ReadOnlySandbox)
    {
        return Err(
            "That harness has no read-only sandbox, so it can only review in a worktree"
                .to_string(),
        );
    }
    if request.focus.contains(&FocusArea::RunTests) && request.step == CheckoutStep::NoCheckout {
        return Err("Running tests needs a review checkout".to_string());
    }
    Ok(())
}

/// The queued runs of the Home that may start now, moved to running: as
/// many as keep `MAX_RUNNING` running. Needs no repository.
pub(crate) async fn claim_agent_reviews(database: &Database) -> Result<Vec<AgentReview>, String> {
    let _claiming = CLAIM.lock().await;
    storage::claim_agent_reviews(database, MAX_RUNNING).await
}

/// Every queued or running run of the Home, oldest first: what the queue
/// resumes after a restart.
pub(crate) async fn active_agent_reviews(database: &Database) -> Result<Vec<AgentReview>, String> {
    storage::active_agent_reviews(database).await
}

impl ReviewDesk {
    /// Queue an agent review of `request.targets`: one PR, or a whole stack
    /// reviewed as one unit, filed under its top PR.
    pub(crate) async fn enqueue_agent_review(
        &self,
        request: AgentReviewRequest,
    ) -> Result<AgentReview, String> {
        self.enqueue(request, None).await
    }

    async fn enqueue(
        &self,
        request: AgentReviewRequest,
        since_sha: Option<String>,
    ) -> Result<AgentReview, String> {
        check_request(&request)?;
        let top = request
            .filed_under()
            .cloned()
            .ok_or_else(|| "Pick a pull request to review".to_string())?;
        let review = AgentReview {
            id: format!("ar-{}", unique_suffix()),
            repo_dir: self.repo_key(),
            repo: self.slug().await?,
            number: top.number,
            request,
            status: AgentReviewStatus::Queued,
            thread_id: None,
            head_sha: top.head_sha,
            since_sha,
            findings: 0,
            parse_error: None,
            error: None,
            created_at: unix_millis(),
            started_at: None,
            finished_at: None,
        };
        storage::insert_agent_review(&self.database, PROVIDER, &review).await?;
        Ok(review)
    }

    /// The repository's agent reviews, newest first.
    pub(crate) async fn agent_reviews(&self) -> Result<Vec<AgentReview>, String> {
        let slug = self.slug().await?;
        storage::list_agent_reviews(&self.database, PROVIDER, &slug, LISTED).await
    }

    /// `claim_agent_reviews` on this desk's database.
    #[cfg(test)]
    pub(crate) async fn claim_agent_reviews(&self) -> Result<Vec<AgentReview>, String> {
        claim_agent_reviews(&self.database).await
    }

    /// Record the thread a claimed run started as.
    pub(crate) async fn agent_review_started(
        &self,
        id: String,
        thread_id: String,
    ) -> Result<(), String> {
        storage::set_agent_review_thread(&self.database, &id, &thread_id).await
    }

    /// The run's first message: the patch (or, on a re-run, only the new
    /// commits), the conversation with comment ids, changed files at head
    /// within a budget, the focus areas, the repository's review rules,
    /// what the reviewer dismissed last time, and the findings contract.
    pub(crate) async fn agent_review_prompt(&self, id: String) -> Result<String, String> {
        let review = self.run(&id).await?;
        let rules: Vec<String> = storage::read_review_rules(&self.database, PROVIDER, &review.repo)
            .await?
            .into_iter()
            .map(|rule| rule.rule)
            .collect();
        let dismissed = self.dismissed_last_time(&review).await?;
        let request = review.request.clone();
        let since_sha = review.since_sha.clone();
        self.blocking("Writing the review prompt failed", move |desk| {
            let prs = request
                .targets
                .iter()
                .map(|target| desk.github.pr_detail(target.number))
                .collect::<Result<Vec<PrDetail>, String>>()?;
            // Best effort: a PRs-only repository has no clone to read from,
            // and the agent can still read the patch.
            let _ = desk.fetch_heads(&request.targets);
            let head = prs.last().map(|pr| pr.head_sha.clone()).unwrap_or_default();
            let since_patch = since_sha.as_deref().and_then(|since| {
                objects::diff_between(&desk.host, desk.dir(), since, &head)
                    .ok()
                    .map(|patch| patch.text)
            });
            let since = match (since_sha.as_deref(), since_patch.as_deref()) {
                (Some(sha), Some(patch)) => Some(SinceLastRun { sha, patch }),
                _ => None,
            };
            let changed = changed_paths(&prs, since_patch.as_deref());
            let (files_at_head, files_left_out) = desk.files_at(&head, changed);
            Ok(agent_review_prompt(&PromptInput {
                request: &request,
                prs: &prs,
                files_at_head: &files_at_head,
                files_left_out: &files_left_out,
                rules: &rules,
                dismissed: &dismissed,
                since,
            }))
        })
        .await
    }

    /// End run `id` with its final message. `None` means the turn failed
    /// (`error` says how). A message without a readable findings block
    /// still ends the run, with `parse_error` set and no Findings.
    pub(crate) async fn record_agent_review_result(
        &self,
        id: String,
        message: Option<String>,
        error: Option<String>,
    ) -> Result<AgentReview, String> {
        let review = self.run(&id).await?;
        let Some(message) = message else {
            let error = error.unwrap_or_else(|| "The agent review stopped".to_string());
            storage::finish_agent_review(
                &self.database,
                &id,
                AgentReviewStatus::Failed,
                0,
                None,
                Some(error),
            )
            .await?;
            return self.run(&id).await;
        };
        match parse_findings_block(&message) {
            Ok(reported) => {
                // Each PR of a stack review with its own head and comments.
                let mut prs = Vec::new();
                for target in &review.request.targets {
                    prs.push(ReviewedPr {
                        number: target.number,
                        head_sha: if target.number == review.number {
                            review.head_sha.clone()
                        } else {
                            target.head_sha.clone()
                        },
                        comment_ids: self.comment_ids(target.number).await,
                    });
                }
                let findings = findings_from_report(&review.id, review.number, &prs, reported);
                storage::write_findings(&self.database, PROVIDER, &review.repo, &id, &findings)
                    .await?;
                let count = i64::try_from(findings.len()).unwrap_or(i64::MAX);
                storage::finish_agent_review(
                    &self.database,
                    &id,
                    AgentReviewStatus::Done,
                    count,
                    None,
                    None,
                )
                .await?;
            }
            Err(parse_error) => {
                storage::finish_agent_review(
                    &self.database,
                    &id,
                    AgentReviewStatus::Done,
                    0,
                    Some(parse_error.to_string()),
                    None,
                )
                .await?;
            }
        }
        self.run(&id).await
    }

    /// Queue run `id` again for `targets` (the PRs at their heads now),
    /// reviewing only the commits since the run's head when the head moved.
    pub(crate) async fn rerun_agent_review(
        &self,
        id: String,
        targets: Vec<CheckoutTarget>,
    ) -> Result<AgentReview, String> {
        let previous = self.run(&id).await?;
        let mut request = previous.request.clone();
        if !targets.is_empty() {
            request.targets = targets;
        }
        let head = request
            .filed_under()
            .map(|target| target.head_sha.clone())
            .unwrap_or_default();
        let since = (head != previous.head_sha).then_some(previous.head_sha);
        self.enqueue(request, since).await
    }

    /// Queue re-runs for PRs whose head moved past their last finished run,
    /// where that run asked for "Re-run when new commits land". `heads` are
    /// the PRs as the inbox has them now. Returns the queued runs.
    pub(crate) async fn rerun_on_new_commits(
        &self,
        heads: Vec<CheckoutTarget>,
    ) -> Result<Vec<AgentReview>, String> {
        let reviews = self.agent_reviews().await?;
        let mut queued = Vec::new();
        for target in heads {
            // Newest first: the PR's latest run decides.
            let Some(latest) = reviews.iter().find(|review| review.number == target.number) else {
                continue;
            };
            let moved = latest.head_sha != target.head_sha;
            if latest.status != AgentReviewStatus::Done
                || !latest.request.rerun_on_new_commits
                || !moved
                || latest.request.targets.len() != 1
            {
                continue;
            }
            queued.push(
                self.rerun_agent_review(latest.id.clone(), vec![target])
                    .await?,
            );
        }
        Ok(queued)
    }

    /// Drop a queued run, or forget a finished one with its Findings.
    pub(crate) async fn remove_agent_review(&self, id: String) -> Result<(), String> {
        storage::delete_agent_review(&self.database, &id).await
    }

    /// PR `number`'s Findings from every run.
    pub(crate) async fn findings(&self, number: i64) -> Result<Vec<Finding>, String> {
        pr_number(number)?;
        let slug = self.slug().await?;
        storage::read_findings(&self.database, PROVIDER, &slug, number).await
    }

    /// Record the reviewer's decision on a Finding: added to the review,
    /// back to a draft, or dismissed with a reason (and a note for
    /// "Wrong, tell the agent why").
    pub(crate) async fn set_finding_state(
        &self,
        id: String,
        state: FindingState,
        reason: Option<DismissReason>,
        note: Option<String>,
    ) -> Result<Finding, String> {
        let (reason, note) = match state {
            FindingState::Dismissed => (
                Some(reason.unwrap_or(DismissReason::NotAProblem)),
                note.filter(|note| !note.trim().is_empty()),
            ),
            _ => (None, None),
        };
        storage::set_finding_state(&self.database, &id, state, reason, note).await?;
        storage::read_finding(&self.database, &id)
            .await?
            .ok_or_else(|| "That finding is gone".to_string())
    }

    pub(crate) async fn review_rules(&self) -> Result<Vec<ReviewRule>, String> {
        let slug = self.slug().await?;
        storage::read_review_rules(&self.database, PROVIDER, &slug).await
    }

    /// Save "Skip this kind of finding" as a repository review rule.
    pub(crate) async fn add_review_rule(&self, rule: String) -> Result<ReviewRule, String> {
        let rule = rule.trim().to_string();
        if rule.is_empty() {
            return Err("Write the rule first".to_string());
        }
        let slug = self.slug().await?;
        let rule = ReviewRule {
            id: format!("rule-{}", unique_suffix()),
            rule,
            created_at: unix_millis(),
        };
        storage::insert_review_rule(&self.database, PROVIDER, &slug, &rule).await?;
        Ok(rule)
    }

    pub(crate) async fn update_review_rule(&self, id: String, rule: String) -> Result<(), String> {
        let rule = rule.trim();
        if rule.is_empty() {
            return Err("Write the rule first".to_string());
        }
        let slug = self.slug().await?;
        storage::update_review_rule(&self.database, PROVIDER, &slug, &id, rule).await
    }

    pub(crate) async fn remove_review_rule(&self, id: String) -> Result<(), String> {
        let slug = self.slug().await?;
        storage::delete_review_rule(&self.database, PROVIDER, &slug, &id).await
    }

    // --- helpers ---------------------------------------------------------------

    async fn run(&self, id: &str) -> Result<AgentReview, String> {
        storage::read_agent_review(&self.database, id)
            .await?
            .ok_or_else(|| "That agent review is gone".to_string())
    }

    /// The Findings the reviewer dismissed on the PR's previous run.
    async fn dismissed_last_time(&self, review: &AgentReview) -> Result<Vec<Finding>, String> {
        let reviews =
            storage::list_agent_reviews(&self.database, PROVIDER, &review.repo, LISTED).await?;
        let Some(previous) = reviews.iter().find(|other| {
            other.number == review.number
                && other.id != review.id
                && other.created_at <= review.created_at
                && other.status == AgentReviewStatus::Done
        }) else {
            return Ok(Vec::new());
        };
        Ok(storage::read_run_findings(&self.database, &previous.id)
            .await?
            .into_iter()
            .filter(|finding| finding.state == FindingState::Dismissed)
            .collect())
    }

    /// The ids of PR `number`'s comments, for `sameAsCommentId`. Empty when
    /// GitHub can't be read: no Finding is merged then.
    async fn comment_ids(&self, number: i64) -> HashSet<i64> {
        self.blocking("Reading the PR failed", move |desk| {
            desk.github.pr_detail(number)
        })
        .await
        .map(|detail| {
            detail
                .comments
                .iter()
                .map(|comment| comment.id)
                .filter(|id| *id > 0)
                .collect()
        })
        .unwrap_or_default()
    }

    fn fetch_heads(&self, targets: &[CheckoutTarget]) -> Result<(), String> {
        let slug = self.github.repo()?;
        let remote = objects::remote_for_slug(&self.host, self.dir(), &slug)?
            .ok_or_else(|| format!("No remote in this repository points at {slug}"))?;
        let numbers = targets
            .iter()
            .map(|target| pr_number(target.number))
            .collect::<Result<Vec<_>, _>>()?;
        objects::fetch_pr_heads(&self.host, self.dir(), &remote, &numbers)?;
        Ok(())
    }

    /// `paths` as they are at `head`, until the budget runs out; the rest by
    /// name.
    fn files_at(&self, head: &str, paths: Vec<String>) -> (Vec<(String, String)>, Vec<String>) {
        let mut included = Vec::new();
        let mut left_out = Vec::new();
        let mut used = 0;
        for path in paths {
            let text = objects::read_blob(&self.host, self.dir(), head, &path)
                .ok()
                .filter(|blob| !blob.truncated)
                .and_then(|blob| blob.text);
            match text {
                Some(text) if used + text.len() <= FILE_BUDGET => {
                    used += text.len();
                    included.push((path, text));
                }
                Some(_) => left_out.push(path),
                // Removed or binary at head: nothing to read.
                None => {}
            }
        }
        (included, left_out)
    }
}

/// The files a review reads at head: the PRs' changed files, or on a
/// re-run only those the new commits touch. In order, without repeats.
fn changed_paths(prs: &[PrDetail], since_patch: Option<&str>) -> Vec<String> {
    let mut seen = HashSet::new();
    let paths: Vec<String> = match since_patch {
        Some(patch) => patch
            .lines()
            .filter_map(|line| line.strip_prefix("+++ b/"))
            .map(str::to_string)
            .collect(),
        None => prs
            .iter()
            .flat_map(|pr| pr.files.iter())
            .filter(|file| file.status != "removed")
            .map(|file| file.path.clone())
            .collect(),
    };
    paths
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{fixture, Fixture};
    use crate::harness::HarnessKind;
    use crate::review::agent_review::{
        AgentReviewRequest, AgentReviewStatus, FocusArea, MAX_RUNNING,
    };
    use crate::review::checkout_types::CheckoutTarget;
    use crate::review::findings::{DismissReason, FindingSeverity, FindingState};
    use crate::review::requirements::CheckoutStep;
    use crate::review::types::{InboxPr, PrComment, PrDetail, PrFile, PrSummary};

    fn target(pr: &InboxPr) -> CheckoutTarget {
        CheckoutTarget {
            number: pr.number,
            head_ref: pr.head_ref.clone(),
            head_sha: pr.head_sha.clone(),
            mine: false,
            stack: None,
        }
    }

    fn request(targets: Vec<CheckoutTarget>) -> AgentReviewRequest {
        AgentReviewRequest {
            targets,
            harness: HarnessKind::Claude,
            step: CheckoutStep::NoCheckout,
            focus: vec![FocusArea::Correctness],
            keep_drafts: true,
            rerun_on_new_commits: false,
        }
    }

    /// PR `number` with one human comment, id 501, as GitHub details it.
    fn detail(fixture: &Fixture, pr: &InboxPr) -> PrDetail {
        let detail = PrDetail {
            summary: PrSummary {
                number: pr.number,
                title: pr.title.clone(),
                author: pr.author.clone(),
                head_ref: pr.head_ref.clone(),
                base_ref: "main".into(),
                ..PrSummary::default()
            },
            head_sha: pr.head_sha.clone(),
            base_sha: pr.base_sha.clone(),
            files: vec![PrFile {
                path: "src/one.rs".into(),
                status: "added".into(),
                patch: "@@ -0,0 +1 @@\n+pub fn one() {}".into(),
                ..PrFile::default()
            }],
            comments: vec![PrComment {
                id: 501,
                author: "mira-k".into(),
                body: "Does this need a test?".into(),
                path: Some("src/one.rs".into()),
                line: Some(1),
                ..PrComment::default()
            }],
            ..PrDetail::default()
        };
        fixture.github.put_detail(pr.number, detail.clone());
        detail
    }

    const FINAL_MESSAGE: &str = "Two things.\n\n```json\n[\
        {\"path\": \"src/one.rs\", \"line\": 1, \"severity\": \"high\", \"title\": \"No test\", \"body\": \"one() has no test.\", \"sameAsCommentId\": 501},\
        {\"path\": \"src/one.rs\", \"line\": 1, \"severity\": \"low\", \"title\": \"Naming\", \"body\": \"one is vague.\"}\
    ]\n```";

    #[tokio::test]
    async fn agent_reviews_queue_and_run_two_at_a_time() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let mut ids = Vec::new();
        for (number, branch, path) in [(1, "a", "a.rs"), (2, "b", "b.rs"), (3, "c", "c.rs")] {
            let pr = fixture.open_pr(number, branch, path);
            let review = fixture
                .desk
                .enqueue_agent_review(request(vec![target(&pr)]))
                .await
                .unwrap();
            assert_eq!(review.status, AgentReviewStatus::Queued);
            ids.push(review.id);
        }

        let first = fixture.desk.claim_agent_reviews().await.unwrap();
        assert_eq!(first.len(), MAX_RUNNING);
        assert_eq!(first[0].number, 1);
        assert!(fixture.desk.claim_agent_reviews().await.unwrap().is_empty());

        fixture
            .desk
            .record_agent_review_result(ids[0].clone(), None, Some("crashed".into()))
            .await
            .unwrap();
        let next = fixture.desk.claim_agent_reviews().await.unwrap();
        assert_eq!(next.iter().map(|r| r.number).collect::<Vec<_>>(), [3]);

        let listed = fixture.desk.agent_reviews().await.unwrap();
        let failed = listed.iter().find(|review| review.id == ids[0]).unwrap();
        assert_eq!(failed.status, AgentReviewStatus::Failed);
        assert_eq!(failed.error.as_deref(), Some("crashed"));
    }

    #[tokio::test]
    async fn a_final_message_becomes_draft_findings_and_a_repeat_of_a_comment_is_merged() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(7, "feature", "src/one.rs");
        detail(&fixture, &pr);
        let review = fixture
            .desk
            .enqueue_agent_review(request(vec![target(&pr)]))
            .await
            .unwrap();
        fixture.desk.claim_agent_reviews().await.unwrap();
        fixture
            .desk
            .agent_review_started(review.id.clone(), "thread-1".into())
            .await
            .unwrap();

        let done = fixture
            .desk
            .record_agent_review_result(review.id.clone(), Some(FINAL_MESSAGE.into()), None)
            .await
            .unwrap();
        assert_eq!(done.status, AgentReviewStatus::Done);
        assert_eq!(done.findings, 2);
        assert_eq!(done.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(done.parse_error, None);

        let findings = fixture.desk.findings(7).await.unwrap();
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].state, FindingState::Merged);
        assert_eq!(findings[0].same_as_comment_id, Some(501));
        assert_eq!(findings[0].severity, FindingSeverity::High);
        assert_eq!(findings[1].state, FindingState::Draft);
        assert_eq!(findings[1].head_sha, pr.head_sha);
        assert!(fixture.desk.findings(8).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_final_message_without_a_findings_block_ends_with_no_findings_parsed() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(7, "feature", "src/one.rs");
        let review = fixture
            .desk
            .enqueue_agent_review(request(vec![target(&pr)]))
            .await
            .unwrap();
        fixture.desk.claim_agent_reviews().await.unwrap();
        let done = fixture
            .desk
            .record_agent_review_result(review.id, Some("Looks fine to me.".into()), None)
            .await
            .unwrap();
        assert_eq!(done.status, AgentReviewStatus::Done);
        assert_eq!(done.findings, 0);
        assert!(done.parse_error.is_some());
        assert!(fixture.desk.findings(7).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_review_needs_a_read_only_sandbox_below_a_worktree_and_tests_need_a_checkout() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(7, "feature", "src/one.rs");
        let mut maki = request(vec![target(&pr)]);
        maki.harness = HarnessKind::Maki;
        assert!(fixture
            .desk
            .enqueue_agent_review(maki.clone())
            .await
            .is_err());
        maki.step = CheckoutStep::Worktree;
        assert!(fixture.desk.enqueue_agent_review(maki).await.is_ok());

        let mut tests = request(vec![target(&pr)]);
        tests.focus.push(FocusArea::RunTests);
        assert!(fixture
            .desk
            .enqueue_agent_review(tests.clone())
            .await
            .is_err());
        tests.step = CheckoutStep::ReviewCheckout;
        assert!(fixture.desk.enqueue_agent_review(tests).await.is_ok());
    }

    #[tokio::test]
    async fn the_prompt_carries_comment_ids_rules_and_the_contract() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(7, "feature", "src/one.rs");
        detail(&fixture, &pr);
        fixture
            .desk
            .add_review_rule("Import order nits".into())
            .await
            .unwrap();
        let review = fixture
            .desk
            .enqueue_agent_review(request(vec![target(&pr)]))
            .await
            .unwrap();
        let prompt = fixture.desk.agent_review_prompt(review.id).await.unwrap();
        assert!(
            prompt.contains("[comment 501] mira-k on src/one.rs:1"),
            "{prompt}"
        );
        assert!(prompt.contains("+pub fn one() {}"));
        assert!(prompt.contains("- Import order nits"));
        assert!(prompt.contains("Do not run anything"));
        // The changed file at head, read from the fetched PR ref.
        assert!(prompt.contains("### src/one.rs"));
        assert!(prompt.contains("```json"));
        assert!(prompt.contains("sameAsCommentId"));
    }

    #[tokio::test]
    async fn a_rerun_on_new_commits_reviews_only_them_and_hears_why_findings_were_dismissed() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let pr = fixture.open_pr(7, "feature", "src/one.rs");
        detail(&fixture, &pr);
        let mut first = request(vec![target(&pr)]);
        first.rerun_on_new_commits = true;
        let review = fixture.desk.enqueue_agent_review(first).await.unwrap();
        fixture.desk.claim_agent_reviews().await.unwrap();
        fixture
            .desk
            .record_agent_review_result(review.id.clone(), Some(FINAL_MESSAGE.into()), None)
            .await
            .unwrap();
        let naming = fixture.desk.findings(7).await.unwrap()[1].id.clone();
        let dismissed = fixture
            .desk
            .set_finding_state(
                naming,
                FindingState::Dismissed,
                Some(DismissReason::Wrong),
                Some("one is the domain term".into()),
            )
            .await
            .unwrap();
        assert_eq!(dismissed.dismiss_reason, Some(DismissReason::Wrong));

        // Nothing moved yet: no re-run.
        assert!(fixture
            .desk
            .rerun_on_new_commits(vec![target(&pr)])
            .await
            .unwrap()
            .is_empty());

        let new_head = fixture.push_to_pr(7, "feature", "src/two.rs");
        let mut moved = target(&pr);
        moved.head_sha = new_head.clone();
        let mut updated = detail(&fixture, &pr);
        updated.head_sha = new_head.clone();
        fixture.github.put_detail(7, updated);

        let queued = fixture
            .desk
            .rerun_on_new_commits(vec![moved.clone()])
            .await
            .unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].since_sha.as_deref(), Some(pr.head_sha.as_str()));
        assert_eq!(queued[0].head_sha, new_head);

        let prompt = fixture
            .desk
            .agent_review_prompt(queued[0].id.clone())
            .await
            .unwrap();
        assert!(
            prompt.contains("New commits since the last review"),
            "{prompt}"
        );
        assert!(prompt.contains("+pub fn two() {}"));
        assert!(!prompt.contains("### Patch"));
        assert!(prompt.contains("Naming. Wrong: one is the domain term"));
        // The merged finding was not dismissed, so it isn't listed.
        assert!(!prompt.contains("No test. "));
    }

    #[tokio::test]
    async fn a_stack_review_is_filed_under_its_top_pr() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let bottom = fixture.open_pr(1, "a", "a.rs");
        let top = fixture.open_pr(2, "b", "b.rs");
        let review = fixture
            .desk
            .enqueue_agent_review(request(vec![target(&bottom), target(&top)]))
            .await
            .unwrap();
        assert_eq!(review.number, 2);
        assert_eq!(review.head_sha, top.head_sha);
        assert_eq!(review.request.targets.len(), 2);
    }

    #[tokio::test]
    async fn a_stack_reviews_findings_belong_to_their_own_pr_and_its_comments() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let bottom = fixture.open_pr(1, "a", "a.rs");
        let top = fixture.open_pr(2, "b", "b.rs");
        // Comment 501 is on the bottom PR only.
        detail(&fixture, &bottom);
        let mut top_detail = detail(&fixture, &top);
        top_detail.comments.clear();
        fixture.github.put_detail(2, top_detail);
        let review = fixture
            .desk
            .enqueue_agent_review(request(vec![target(&bottom), target(&top)]))
            .await
            .unwrap();
        fixture.desk.claim_agent_reviews().await.unwrap();
        let message = "```json\n[\
            {\"pr\": 1, \"path\": \"a.rs\", \"line\": 1, \"severity\": \"high\", \"title\": \"Repeat\", \"body\": \"x\", \"sameAsCommentId\": 501},\
            {\"pr\": 2, \"path\": \"b.rs\", \"line\": 1, \"severity\": \"low\", \"title\": \"New\", \"body\": \"y\"}\
        ]\n```";

        fixture
            .desk
            .record_agent_review_result(review.id, Some(message.into()), None)
            .await
            .unwrap();

        let lower = fixture.desk.findings(1).await.unwrap();
        assert_eq!(lower.len(), 1);
        assert_eq!(lower[0].state, FindingState::Merged);
        assert_eq!(lower[0].same_as_comment_id, Some(501));
        assert_eq!(lower[0].head_sha, bottom.head_sha);
        let upper = fixture.desk.findings(2).await.unwrap();
        assert_eq!(upper[0].head_sha, top.head_sha);
    }

    #[tokio::test]
    async fn review_rules_are_kept_per_repository_and_editable() {
        let Some(fixture) = fixture().await else {
            return;
        };
        let rule = fixture
            .desk
            .add_review_rule("  Import order nits ".into())
            .await
            .unwrap();
        assert_eq!(rule.rule, "Import order nits");
        fixture
            .desk
            .update_review_rule(rule.id.clone(), "Import order".into())
            .await
            .unwrap();
        let rules = fixture.desk.review_rules().await.unwrap();
        assert_eq!(
            rules.iter().map(|r| r.rule.as_str()).collect::<Vec<_>>(),
            ["Import order"]
        );
        fixture.desk.remove_review_rule(rule.id).await.unwrap();
        assert!(fixture.desk.review_rules().await.unwrap().is_empty());
        assert!(fixture.desk.add_review_rule(" ".into()).await.is_err());
    }
}
