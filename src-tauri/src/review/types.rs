//! Provider-neutral wire types for the review view.
//!
//! Deliberately not GitHub-shaped: a future adapter (GitLab, Gitea) would map
//! its own API onto these same structs so the frontend never changes.

use serde::{Deserialize, Serialize};

/// Availability and auth state of the active review provider.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderStatus {
    /// The `gh` executable is on PATH.
    pub(crate) installed: bool,
    /// `gh auth status` reports a logged-in account.
    pub(crate) authenticated: bool,
    /// An actionable message when the provider is not ready.
    pub(crate) message: Option<String>,
}

/// A single open pull request in the PR picker.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrSummary {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) author: String,
    /// `OPEN`, `CLOSED`, or `MERGED`.
    pub(crate) state: String,
    pub(crate) is_draft: bool,
    pub(crate) base_ref: String,
    pub(crate) head_ref: String,
    pub(crate) updated_at: String,
    pub(crate) url: String,
}

/// One commit on the PR branch.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrCommit {
    pub(crate) oid: String,
    pub(crate) short_oid: String,
    pub(crate) headline: String,
    pub(crate) author: String,
}

/// One line inside a parsed diff hunk, carrying stable old/new line numbers so
/// the UI can anchor an inline comment to an exact side and line.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffLine {
    /// `context`, `add`, or `del`.
    pub(crate) kind: String,
    pub(crate) content: String,
    /// Line number on the old (base) side, when present.
    pub(crate) old_line: Option<i64>,
    /// Line number on the new (head) side, when present.
    pub(crate) new_line: Option<i64>,
}

/// A single `@@` hunk of a file's diff with parsed lines and stable anchors.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffHunk {
    pub(crate) header: String,
    pub(crate) old_start: i64,
    pub(crate) old_lines: i64,
    pub(crate) new_start: i64,
    pub(crate) new_lines: i64,
    pub(crate) lines: Vec<DiffLine>,
}

/// One changed file in a PR (or a local branch diff).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrFile {
    pub(crate) path: String,
    /// The pre-rename path when `status` is `renamed`.
    pub(crate) old_path: Option<String>,
    /// `added`, `modified`, `removed`, or `renamed`.
    pub(crate) status: String,
    pub(crate) additions: i64,
    pub(crate) deletions: i64,
    /// Raw unified diff for the file, fed to `DiffBlock` for rendering.
    pub(crate) patch: String,
    pub(crate) hunks: Vec<DiffHunk>,
    /// Whether the file's patch was omitted (binary, or too large).
    pub(crate) patch_truncated: bool,
    /// The file's blob id at the head, when the provider gives one. Viewed
    /// state is keyed by it, so a file that changes reads as unviewed.
    pub(crate) blob_sha: Option<String>,
}

/// One review or conversation comment, flattened; the frontend groups inline
/// comments into threads by `thread_id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrComment {
    /// REST id (`databaseId` for an inline comment, from the URL for a
    /// conversation comment); 0 when GitHub gave neither.
    pub(crate) id: i64,
    pub(crate) author: String,
    pub(crate) body: String,
    pub(crate) created_at: String,
    /// File path for an inline comment; `None` for a conversation comment.
    pub(crate) path: Option<String>,
    /// Line on the diff the comment anchors to.
    pub(crate) line: Option<i64>,
    /// First line of a multi-line comment at the head; `None` once outdated.
    pub(crate) start_line: Option<i64>,
    /// `line` on the commit the comment was made on; `diff_hunk` counts
    /// from there.
    pub(crate) original_line: Option<i64>,
    /// `start_line` on the commit the comment was made on.
    pub(crate) original_start_line: Option<i64>,
    /// The diff hunk around the comment on the commit it was made on,
    /// ending at `original_line` and numbered like it. Shown above a thread
    /// away from the diff.
    pub(crate) diff_hunk: Option<DiffHunk>,
    /// Whether the commented lines changed since, so the thread no longer
    /// sits on the head diff.
    pub(crate) outdated: bool,
    /// `RIGHT` (head) or `LEFT` (base) for an inline comment.
    pub(crate) side: Option<String>,
    /// The review-thread node id (GraphQL), shared by every comment in a thread.
    pub(crate) thread_id: Option<String>,
    /// Whether the owning review thread is resolved.
    pub(crate) is_resolved: bool,
    /// The node id of the review an inline comment was posted in; matches
    /// `PrReview::id`. `None` for a conversation comment.
    pub(crate) review_id: Option<String>,
}

/// One submitted review: its verdict and summary body. Inline comments carry
/// the review's id in `PrComment::review_id`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrReview {
    /// The review's node id.
    pub(crate) id: String,
    pub(crate) author: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED` or `DISMISSED`.
    pub(crate) state: String,
    pub(crate) body: String,
    pub(crate) submitted_at: String,
}

/// A compact rollup of the head commit's status checks.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChecksSummary {
    pub(crate) total: i64,
    pub(crate) passing: i64,
    pub(crate) failing: i64,
    pub(crate) pending: i64,
}

/// The full review payload for one PR.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrDetail {
    pub(crate) summary: PrSummary,
    pub(crate) body: String,
    pub(crate) head_sha: String,
    /// The tip of the base git branch when the PR was read.
    pub(crate) base_sha: String,
    pub(crate) commits: Vec<PrCommit>,
    pub(crate) files: Vec<PrFile>,
    pub(crate) comments: Vec<PrComment>,
    /// Submitted reviews, oldest first; the viewer's pending one is left out.
    pub(crate) reviews: Vec<PrReview>,
    pub(crate) checks: Option<ChecksSummary>,
    /// True when the changed-file list was capped at `MAX_FILES`.
    pub(crate) files_truncated: bool,
}

/// Result of comparing a locally-open PR against the current remote.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrFreshness {
    pub(crate) stale: bool,
    pub(crate) remote_head: String,
    pub(crate) remote_updated_at: String,
}

/// One pending inline comment to attach to a submitted review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PendingComment {
    pub(crate) path: String,
    pub(crate) line: i64,
    #[serde(default)]
    pub(crate) side: Option<String>,
    /// The first line of a multi-line comment; `line` is its last.
    #[serde(default)]
    pub(crate) start_line: Option<i64>,
    #[serde(default)]
    pub(crate) start_side: Option<String>,
    pub(crate) body: String,
}

/// One open pull request in a repository's inbox, with everything the inbox
/// row and its filters need, from one GraphQL query.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InboxPr {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) author: String,
    pub(crate) url: String,
    pub(crate) is_draft: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) head_ref: String,
    /// The head commit. Every diff and file read for this PR is keyed on it.
    pub(crate) head_sha: String,
    pub(crate) base_ref: String,
    pub(crate) base_sha: String,
    /// `owner/name` of the repository the head branch lives in; differs from
    /// the inbox's repository for a fork PR. `None` when the fork is deleted.
    pub(crate) head_repo: Option<String>,
    /// The head repository's owner may push to the branch only if the author
    /// allowed it.
    pub(crate) maintainer_can_modify: bool,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or none.
    pub(crate) review_decision: Option<String>,
    /// Logins and team slugs asked to review.
    pub(crate) review_requests: Vec<String>,
    /// Each reviewer's latest approving or change-requesting review.
    pub(crate) reviews: Vec<InboxReview>,
    /// The head commit's combined check state: `SUCCESS`, `FAILURE`,
    /// `PENDING`, `ERROR`, `EXPECTED`, or none when there are no checks.
    pub(crate) checks: Option<String>,
    pub(crate) unresolved_threads: i64,
    pub(crate) additions: i64,
    pub(crate) deletions: i64,
    pub(crate) changed_files: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InboxReview {
    pub(crate) author: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`, `DISMISSED`.
    pub(crate) state: String,
    /// The head commit the review was written against, when known.
    #[serde(default)]
    pub(crate) commit: Option<String>,
}

/// What the last query cost against GitHub's GraphQL rate limit.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RateLimit {
    pub(crate) cost: i64,
    pub(crate) remaining: i64,
    pub(crate) reset_at: String,
}

/// Something that changed on a PR since the previous sync, for the "since you
/// last looked" feed. Derived by comparing two inbox snapshots, so it costs no
/// extra requests and needs no notification permission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InboxEvent {
    pub(crate) number: i64,
    /// `opened`, `pushed`, `reviewed`, `reviewRequested`, `checksFailed`,
    /// `checksPassed`, `closed`.
    pub(crate) kind: String,
    /// Who did it, when the snapshot says.
    pub(crate) actor: Option<String>,
    /// The new state for `reviewed` (`APPROVED`, `CHANGES_REQUESTED`, ...).
    pub(crate) state: Option<String>,
}

/// A repository's inbox: its open PRs as of `synced_at`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Inbox {
    /// `owner/name`.
    pub(crate) repo: String,
    /// The signed-in GitHub login, for "needs me" and "mine".
    pub(crate) viewer: String,
    pub(crate) prs: Vec<InboxPr>,
    /// Changes since the previous sync; empty on the first.
    pub(crate) events: Vec<InboxEvent>,
    pub(crate) rate_limit: Option<RateLimit>,
    /// False when the repository has more open PRs than one sync reads; the
    /// most recently updated ones are listed.
    pub(crate) complete: bool,
    /// Unix milliseconds.
    pub(crate) synced_at: i64,
    /// How each PR in `prs` stands for the viewer, in the same order.
    pub(crate) triage: Vec<PrTriage>,
    /// The filter counts, from `triage`.
    pub(crate) counts: InboxCounts,
    /// "Since you last looked": activity on these PRs that the viewer has
    /// not marked seen, newest first. Unlike `events`, it builds up across
    /// syncs until marked seen.
    pub(crate) feed: Vec<FeedEntry>,
}

/// Why a PR needs the viewer: the first reason that holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NeedsMe {
    /// My review is requested and I have not reviewed its current head.
    ReviewRequested,
    /// Mine, and a reviewer requested changes.
    ChangesRequested,
    /// Mine, and its checks fail.
    ChecksFailing,
    /// Mine, with unresolved review threads.
    Unresolved,
    /// Mine, approved, out of draft and green.
    ReadyToMerge,
}

/// How one inbox PR stands for the viewer. The inbox filters, the sidebar's
/// "N need you" badge and the row's primary action all read this.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrTriage {
    pub(crate) number: i64,
    /// The viewer wrote it.
    pub(crate) mine: bool,
    /// The viewer's review is asked for: requested now, or reviewed before
    /// and pushed to since.
    pub(crate) review_requested: bool,
    /// `None` when it does not need the viewer (or is muted).
    pub(crate) needs_me: Option<NeedsMe>,
    /// The viewer's latest approving or change-requesting review state.
    pub(crate) my_review: Option<String>,
    /// Commits on the head since the viewer's latest review, when that
    /// review's commit is known and both commits are in the clone.
    pub(crate) commits_since_my_review: Option<i64>,
    /// Feed entries on this PR not yet marked seen: the row's "N new".
    pub(crate) new_activity: i64,
    /// Muted until new commits land: left out of Needs me.
    pub(crate) muted: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct InboxCounts {
    pub(crate) needs_me: i64,
    pub(crate) mine: i64,
    pub(crate) review_requested: i64,
    pub(crate) everyone: i64,
}

/// One "since you last looked" entry: an `InboxEvent` kept until seen.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedEntry {
    pub(crate) id: String,
    pub(crate) number: i64,
    /// As `InboxEvent::kind`.
    pub(crate) kind: String,
    pub(crate) actor: Option<String>,
    pub(crate) state: Option<String>,
    /// When the sync that noticed it ran, Unix milliseconds.
    pub(crate) at: i64,
}

/// A stack of pull requests as GitHub records it (native stacked PRs), bottom
/// first: each PR's base is the head of the one before it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrStack {
    /// GitHub's stack number, shown in its stack UI.
    pub(crate) number: i64,
    /// The branch the bottom PR merges into.
    pub(crate) base_ref: String,
    pub(crate) open: bool,
    pub(crate) prs: Vec<StackPr>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackPr {
    pub(crate) number: i64,
    /// `open` or `closed`.
    pub(crate) state: String,
    pub(crate) is_draft: bool,
    pub(crate) merged: bool,
    pub(crate) head_ref: String,
    pub(crate) head_sha: String,
}

/// Whether a repository has native stacks, and the stacks if so.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StackListing {
    /// False when GitHub does not offer stacks for this repository; the inbox
    /// then shows its PRs ungrouped.
    pub(crate) supported: bool,
    pub(crate) stacks: Vec<PrStack>,
}

/// What a PR's head changed since the reviewer last marked it seen.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChangesSince {
    /// The head last marked seen; `None` if never.
    pub(crate) seen_sha: Option<String>,
    pub(crate) head_sha: String,
    /// The seen head is an ancestor of the current one: `files` is exactly
    /// the new commits. Otherwise the branch was rewritten (force-push or
    /// rebase), `files` may include changes that came from the base branch,
    /// and `range_diff` says commit by commit what changed.
    pub(crate) fast_forward: bool,
    pub(crate) files: Vec<PrFile>,
    pub(crate) range_diff: Option<String>,
    pub(crate) truncated: bool,
}

/// One file at one commit, read from the object database.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrFileContents {
    /// `None` for a binary file.
    pub(crate) text: Option<String>,
    pub(crate) size: i64,
    pub(crate) truncated: bool,
}

/// A PR head fetched into the local repository.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FetchedPr {
    pub(crate) number: i64,
    /// `None` when the remote has no such PR.
    pub(crate) head_sha: Option<String>,
}

/// A diff between two commits, read from the object database.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ShaDiff {
    pub(crate) files: Vec<PrFile>,
    /// The patch passed the read cap; the last files are missing or cut.
    pub(crate) truncated: bool,
}

// --- The GitHub provider port's inputs and outputs (see `provider`) ---

/// One inbox read from the provider: who is signed in, the open PRs, and
/// what the read cost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct InboxRead {
    /// `owner/name`.
    pub(crate) repo: String,
    pub(crate) viewer: String,
    pub(crate) prs: Vec<InboxPr>,
    pub(crate) rate_limit: Option<RateLimit>,
    /// False when the repository has more open PRs than one read returns.
    pub(crate) complete: bool,
}

/// A PR's head as the provider has it now, for the stale-head check.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PrHead {
    pub(crate) head_sha: String,
    pub(crate) updated_at: String,
}

/// The verdict a submitted review carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ReviewVerdict {
    #[default]
    Comment,
    Approve,
    RequestChanges,
}

impl ReviewVerdict {
    /// The UI's event name. Anything unrecognised is a plain comment, the
    /// only verdict that cannot approve or block by accident.
    pub(crate) fn from_event(event: &str) -> Self {
        match event {
            "approve" | "APPROVE" => Self::Approve,
            "request-changes" | "REQUEST_CHANGES" | "requestChanges" => Self::RequestChanges,
            _ => Self::Comment,
        }
    }
}

/// One review, posted to the provider in one call.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewSubmission {
    pub(crate) verdict: ReviewVerdict,
    /// The summary.
    pub(crate) body: String,
    /// The head the reviewer read. The review and its comments stay on it
    /// even if the author pushes before submit.
    pub(crate) commit_id: Option<String>,
    pub(crate) comments: Vec<PendingComment>,
    /// Replies to existing review threads, posted inside this review.
    #[serde(default)]
    pub(crate) replies: Vec<ReviewReply>,
}

/// A reply to an existing review thread, posted as part of a review.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewReply {
    /// The review thread's node id.
    pub(crate) thread_id: String,
    pub(crate) body: String,
}

/// A comment to edit or delete. GitHub keeps inline review comments and
/// conversation comments in different collections.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "kind", content = "id")]
pub(crate) enum CommentRef {
    /// An inline comment in a review thread.
    Review(i64),
    /// A comment in the PR's conversation.
    Conversation(i64),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum MergeMethod {
    #[default]
    Merge,
    Squash,
    Rebase,
}

/// A merge, refused by the provider when the PR's head is no longer
/// `head_sha`, so it never merges commits nobody saw.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MergeRequest {
    pub(crate) method: MergeMethod,
    pub(crate) head_sha: Option<String>,
}

/// A pull request to open, for "Add git branch on top" and "Split".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NewPr {
    /// The git branch it merges into.
    pub(crate) base: String,
    /// The git branch with its commits, already pushed.
    pub(crate) head: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) draft: bool,
}

/// A failing check's log, as the check finished; never streamed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CheckLog {
    pub(crate) name: String,
    pub(crate) log: String,
    pub(crate) truncated: bool,
}

/// A repository on GitHub the signed-in account can see, for Add repository.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitHubRepo {
    /// `owner/name`.
    pub(crate) slug: String,
    pub(crate) private: bool,
    pub(crate) fork: bool,
    /// The upstream's `owner/name` for a fork, when GitHub reports it.
    pub(crate) parent: Option<String>,
    pub(crate) default_branch: String,
    pub(crate) clone_url: String,
    pub(crate) ssh_url: String,
}

/// The GitHub account the provider signs in as.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GitHubAccount {
    /// The active login; `None` when signed out.
    pub(crate) login: Option<String>,
    /// Where its token comes from, e.g. `keyring` or `GH_TOKEN`.
    pub(crate) token_source: Option<String>,
    /// Every signed-in login, the active one included, to switch between.
    pub(crate) logins: Vec<String>,
}

/// A repository with open PRs that ask the signed-in account for review.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReviewRequests {
    /// `owner/name`.
    pub(crate) slug: String,
    pub(crate) count: i64,
}

/// A changed file and its blob id at the head, which viewed state is keyed
/// by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileBlob {
    pub(crate) path: String,
    pub(crate) blob_sha: String,
}
