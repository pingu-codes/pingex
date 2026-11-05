//! Agent reviews, their Findings and the repository review rules, per
//! profile.
//!
//! `review_agent_reviews` is also the queue: `claim_agent_reviews` moves the
//! oldest queued reviews to running while fewer than the limit run, across
//! every repository of the Home. `review_findings` holds each run's
//! Findings as JSON, with the reviewer's decision (state, dismiss reason)
//! in columns of its own.

use turso::{params, Database, Row};

use super::db;
use crate::review::agent_review::{AgentReview, AgentReviewRequest, AgentReviewStatus, ReviewRule};
use crate::review::findings::{DismissReason, Finding, FindingState};
use crate::util::time::unix_millis;

const REVIEW_COLUMNS: &str = "id, repo_key, repo, pr_number, request, status, thread_id, head_sha,
     since_sha, findings, parse_error, error, created_at, started_at, finished_at";

fn review_row(row: &Row) -> Result<AgentReview, String> {
    let request: AgentReviewRequest =
        serde_json::from_str(&db::text(row, 4)?).map_err(|error| error.to_string())?;
    Ok(AgentReview {
        id: db::text(row, 0)?,
        repo_dir: db::text(row, 1)?,
        repo: db::text(row, 2)?,
        number: db::int(row, 3)?,
        request,
        status: AgentReviewStatus::parse(&db::text(row, 5)?),
        thread_id: db::opt_text(row, 6)?,
        head_sha: db::text(row, 7)?,
        since_sha: db::opt_text(row, 8)?,
        findings: db::int(row, 9)?,
        parse_error: db::opt_text(row, 10)?,
        error: db::opt_text(row, 11)?,
        created_at: db::int(row, 12)?,
        started_at: db::opt_int(row, 13)?,
        finished_at: db::opt_int(row, 14)?,
    })
}

pub(crate) async fn insert_agent_review(
    database: &Database,
    provider: &str,
    review: &AgentReview,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let request = serde_json::to_string(&review.request).map_err(|error| error.to_string())?;
    db::exec(
        &connection,
        "INSERT INTO review_agent_reviews(id, provider, repo, repo_key, pr_number, request, status,
             thread_id, head_sha, since_sha, findings, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, ?, 0, ?)",
        params![
            review.id.as_str(),
            provider,
            review.repo.as_str(),
            review.repo_dir.as_str(),
            review.number,
            request,
            review.status.as_str(),
            review.head_sha.as_str(),
            review.since_sha.clone(),
            review.created_at
        ],
    )
    .await
}

pub(crate) async fn read_agent_review(
    database: &Database,
    id: &str,
) -> Result<Option<AgentReview>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        &format!("SELECT {REVIEW_COLUMNS} FROM review_agent_reviews WHERE id = ?"),
        params![id],
        review_row,
    )
    .await
}

/// The repository's agent reviews, newest first.
pub(crate) async fn list_agent_reviews(
    database: &Database,
    provider: &str,
    repo: &str,
    limit: i64,
) -> Result<Vec<AgentReview>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {REVIEW_COLUMNS} FROM review_agent_reviews
             WHERE provider = ? AND repo = ?
             ORDER BY created_at DESC, id DESC LIMIT ?"
        ),
        params![provider, repo, limit],
        review_row,
    )
    .await
}

/// Every queued or running agent review of the Home, oldest first.
pub(crate) async fn active_agent_reviews(database: &Database) -> Result<Vec<AgentReview>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {REVIEW_COLUMNS} FROM review_agent_reviews
             WHERE status IN ('queued', 'running')
             ORDER BY created_at, id"
        ),
        (),
        review_row,
    )
    .await
}

/// Move the oldest queued reviews to running while fewer than `limit` run,
/// and return them. Callers serialize claims (the desk holds a lock).
pub(crate) async fn claim_agent_reviews(
    database: &Database,
    limit: usize,
) -> Result<Vec<AgentReview>, String> {
    let active = active_agent_reviews(database).await?;
    let running = active
        .iter()
        .filter(|review| review.status == AgentReviewStatus::Running)
        .count();
    let free = limit.saturating_sub(running);
    let connection = db::conn(database)?;
    let now = unix_millis();
    let mut claimed = Vec::new();
    for mut review in active
        .into_iter()
        .filter(|review| review.status == AgentReviewStatus::Queued)
        .take(free)
    {
        db::exec(
            &connection,
            "UPDATE review_agent_reviews SET status = 'running', started_at = ? WHERE id = ?",
            params![now, review.id.as_str()],
        )
        .await?;
        review.status = AgentReviewStatus::Running;
        review.started_at = Some(now);
        claimed.push(review);
    }
    Ok(claimed)
}

pub(crate) async fn set_agent_review_thread(
    database: &Database,
    id: &str,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_agent_reviews SET thread_id = ? WHERE id = ?",
        params![thread_id, id],
    )
    .await
}

/// End a run: `done` with its Finding count (and why none could be read),
/// or `failed` with an error.
pub(crate) async fn finish_agent_review(
    database: &Database,
    id: &str,
    status: AgentReviewStatus,
    findings: i64,
    parse_error: Option<String>,
    error: Option<String>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_agent_reviews
         SET status = ?, findings = ?, parse_error = ?, error = ?, finished_at = ?
         WHERE id = ?",
        params![
            status.as_str(),
            findings,
            parse_error,
            error,
            unix_millis(),
            id
        ],
    )
    .await
}

pub(crate) async fn delete_agent_review(database: &Database, id: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_findings WHERE agent_review_id = ?",
        params![id],
    )
    .await?;
    db::exec(
        &connection,
        "DELETE FROM review_agent_reviews WHERE id = ?",
        params![id],
    )
    .await
}

// --- Findings ----------------------------------------------------------------

fn finding_row(row: &Row) -> Result<Finding, String> {
    let mut finding: Finding =
        serde_json::from_str(&db::text(row, 0)?).map_err(|error| error.to_string())?;
    finding.state = FindingState::parse(&db::text(row, 1)?);
    finding.dismiss_reason = db::opt_text(row, 2)?.and_then(|value| DismissReason::parse(&value));
    finding.dismiss_note = db::opt_text(row, 3)?;
    Ok(finding)
}

/// Store one run's Findings, replacing any it had.
pub(crate) async fn write_findings(
    database: &Database,
    provider: &str,
    repo: &str,
    agent_review_id: &str,
    findings: &[Finding],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_findings WHERE agent_review_id = ?",
        params![agent_review_id],
    )
    .await?;
    let now = unix_millis();
    for (ordinal, finding) in findings.iter().enumerate() {
        let payload = serde_json::to_string(finding).map_err(|error| error.to_string())?;
        db::exec(
            &connection,
            "INSERT INTO review_findings(id, agent_review_id, provider, repo, pr_number, ordinal,
                 payload, state, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                finding.id.as_str(),
                agent_review_id,
                provider,
                repo,
                finding.pr_number,
                i64::try_from(ordinal).unwrap_or(i64::MAX),
                payload,
                finding.state.as_str(),
                now
            ],
        )
        .await?;
    }
    Ok(())
}

/// PR `pr_number`'s Findings from every run, oldest run first.
pub(crate) async fn read_findings(
    database: &Database,
    provider: &str,
    repo: &str,
    pr_number: i64,
) -> Result<Vec<Finding>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT f.payload, f.state, f.dismiss_reason, f.dismiss_note
         FROM review_findings f JOIN review_agent_reviews r ON r.id = f.agent_review_id
         WHERE f.provider = ? AND f.repo = ? AND f.pr_number = ?
         ORDER BY r.created_at, r.id, f.ordinal",
        params![provider, repo, pr_number],
        finding_row,
    )
    .await
}

/// One run's Findings.
pub(crate) async fn read_run_findings(
    database: &Database,
    agent_review_id: &str,
) -> Result<Vec<Finding>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT payload, state, dismiss_reason, dismiss_note FROM review_findings
         WHERE agent_review_id = ? ORDER BY ordinal",
        params![agent_review_id],
        finding_row,
    )
    .await
}

pub(crate) async fn read_finding(database: &Database, id: &str) -> Result<Option<Finding>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        "SELECT payload, state, dismiss_reason, dismiss_note FROM review_findings WHERE id = ?",
        params![id],
        finding_row,
    )
    .await
}

pub(crate) async fn set_finding_state(
    database: &Database,
    id: &str,
    state: FindingState,
    reason: Option<DismissReason>,
    note: Option<String>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_findings
         SET state = ?, dismiss_reason = ?, dismiss_note = ?, updated_at = ?
         WHERE id = ?",
        params![
            state.as_str(),
            reason.map(DismissReason::as_str),
            note,
            unix_millis(),
            id
        ],
    )
    .await
}

// --- Repository review rules ---------------------------------------------------

pub(crate) async fn read_review_rules(
    database: &Database,
    provider: &str,
    repo: &str,
) -> Result<Vec<ReviewRule>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT id, rule, created_at FROM review_rules
         WHERE provider = ? AND repo = ? ORDER BY created_at, id",
        params![provider, repo],
        |row| {
            Ok(ReviewRule {
                id: db::text(row, 0)?,
                rule: db::text(row, 1)?,
                created_at: db::int(row, 2)?,
            })
        },
    )
    .await
}

pub(crate) async fn insert_review_rule(
    database: &Database,
    provider: &str,
    repo: &str,
    rule: &ReviewRule,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO review_rules(id, provider, repo, rule, created_at) VALUES (?, ?, ?, ?, ?)",
        params![
            rule.id.as_str(),
            provider,
            repo,
            rule.rule.as_str(),
            rule.created_at
        ],
    )
    .await
}

pub(crate) async fn update_review_rule(
    database: &Database,
    provider: &str,
    repo: &str,
    id: &str,
    rule: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE review_rules SET rule = ? WHERE provider = ? AND repo = ? AND id = ?",
        params![rule, provider, repo, id],
    )
    .await
}

pub(crate) async fn delete_review_rule(
    database: &Database,
    provider: &str,
    repo: &str,
    id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM review_rules WHERE provider = ? AND repo = ? AND id = ?",
        params![provider, repo, id],
    )
    .await
}
