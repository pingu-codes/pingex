//! What each turn cost, and where the tokens went.
//!
//! Both harnesses report token usage as a running total per thread; nothing
//! keeps it once the process is gone, and neither says which part of the
//! prompt the tokens paid for. So every turn is recorded here as it ends,
//! with the exact wire figures and an attribution of them across the
//! categories the usage views draw (`features/15-usage.md`). Project and
//! global breakdowns are sums over these rows.

use turso::{params, Database};

use super::db;

/// The `turn_id` of the one row a thread gets for usage that happened before
/// it was tracked: Codex replays a thread's running total when it is resumed,
/// and whatever the total exceeds the rows by is booked here, unattributed.
pub(crate) const OPENING_TURN_ID: &str = "_opening";

/// The wire-level token figures, as both harnesses report them. `input`
/// includes `cached_input` (Codex semantics; the Claude driver adds them up
/// the same way).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct UsageTokens {
    pub(crate) input: u64,
    pub(crate) cached_input: u64,
    pub(crate) cache_write_input: u64,
    pub(crate) output: u64,
    pub(crate) reasoning_output: u64,
}

impl UsageTokens {
    pub(crate) fn total(&self) -> u64 {
        self.input + self.output
    }

    pub(crate) fn add(&mut self, other: &UsageTokens) {
        self.input += other.input;
        self.cached_input += other.cached_input;
        self.cache_write_input += other.cache_write_input;
        self.output += other.output;
        self.reasoning_output += other.reasoning_output;
    }

    /// `self − other`, clamped at zero per field.
    pub(crate) fn saturating_sub(&self, other: &UsageTokens) -> UsageTokens {
        UsageTokens {
            input: self.input.saturating_sub(other.input),
            cached_input: self.cached_input.saturating_sub(other.cached_input),
            cache_write_input: self
                .cache_write_input
                .saturating_sub(other.cache_write_input),
            output: self.output.saturating_sub(other.output),
            reasoning_output: self.reasoning_output.saturating_sub(other.reasoning_output),
        }
    }

    pub(crate) fn is_zero(&self) -> bool {
        self.total() == 0
    }
}

/// Tokens by what they paid for. The four prompt categories are estimates;
/// `output` and `reasoning` are exact; `unattributed` is what could not be
/// placed (an opening row, or context the estimate does not account for).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CategoryTokens {
    pub(crate) system: u64,
    pub(crate) skills: u64,
    pub(crate) user: u64,
    pub(crate) tool: u64,
    pub(crate) output: u64,
    pub(crate) reasoning: u64,
    pub(crate) unattributed: u64,
}

impl CategoryTokens {
    pub(crate) fn total(&self) -> u64 {
        self.system
            + self.skills
            + self.user
            + self.tool
            + self.output
            + self.reasoning
            + self.unattributed
    }
}

/// One turn's usage, as stored.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TurnUsageRow {
    pub(crate) thread_id: String,
    pub(crate) turn_id: String,
    pub(crate) harness: String,
    pub(crate) model: Option<String>,
    pub(crate) tokens: UsageTokens,
    /// Harness-reported cost; `None` when the harness prices nothing (Codex).
    pub(crate) cost_usd: Option<f64>,
    /// Size of the context after this turn's last request.
    pub(crate) context_tokens: Option<u64>,
    pub(crate) context_window: Option<u64>,
    pub(crate) attribution: CategoryTokens,
    pub(crate) recorded_at: i64,
}

/// Which rows a breakdown sums.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UsageScope {
    Thread(String),
    /// Every thread whose `cwd` is the project path or under it.
    Project(String),
    Global,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct UsageTotals {
    pub(crate) tokens: UsageTokens,
    pub(crate) attribution: CategoryTokens,
    /// Sum of the harness-reported costs, or `None` when no row carried one.
    pub(crate) cost_usd: Option<f64>,
    /// Whether some tokens have no reported cost and need an estimate.
    pub(crate) unpriced: bool,
    pub(crate) turns: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModelUsageRow {
    pub(crate) model: Option<String>,
    pub(crate) harness: String,
    pub(crate) tokens: UsageTokens,
    pub(crate) cost_usd: Option<f64>,
    pub(crate) turns: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ThreadUsageRow {
    pub(crate) thread_id: String,
    pub(crate) title: Option<String>,
    pub(crate) tokens: UsageTokens,
    pub(crate) attribution: CategoryTokens,
    pub(crate) cost_usd: Option<f64>,
    pub(crate) last_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct StoredUsageBreakdown {
    pub(crate) totals: UsageTotals,
    pub(crate) by_model: Vec<ModelUsageRow>,
    /// Heaviest threads first; only for project and global scope.
    pub(crate) by_thread: Vec<ThreadUsageRow>,
}

/// How many threads a project or global breakdown lists.
const BY_THREAD_LIMIT: i64 = 25;

const TOKEN_SUMS: &str = "COALESCE(SUM(input_tokens), 0), COALESCE(SUM(cached_input_tokens), 0),
     COALESCE(SUM(cache_write_input_tokens), 0), COALESCE(SUM(output_tokens), 0),
     COALESCE(SUM(reasoning_output_tokens), 0)";

const ATTR_SUMS: &str = "COALESCE(SUM(attr_system), 0), COALESCE(SUM(attr_skills), 0),
     COALESCE(SUM(attr_user), 0), COALESCE(SUM(attr_tool), 0), COALESCE(SUM(attr_output), 0),
     COALESCE(SUM(attr_reasoning), 0), COALESCE(SUM(attr_unattributed), 0)";

/// `SUM(cost_usd)` is null when every row is null, which is exactly the
/// "nothing reported" case; the second column says whether any row without a
/// cost carried tokens.
const COST_SUMS: &str = "SUM(cost_usd),
     COALESCE(MAX(CASE WHEN cost_usd IS NULL AND input_tokens + output_tokens > 0 THEN 1 ELSE 0 END), 0)";

fn tokens_at(row: &turso::Row, at: usize) -> Result<UsageTokens, String> {
    Ok(UsageTokens {
        input: db::int(row, at)? as u64,
        cached_input: db::int(row, at + 1)? as u64,
        cache_write_input: db::int(row, at + 2)? as u64,
        output: db::int(row, at + 3)? as u64,
        reasoning_output: db::int(row, at + 4)? as u64,
    })
}

fn attribution_at(row: &turso::Row, at: usize) -> Result<CategoryTokens, String> {
    Ok(CategoryTokens {
        system: db::int(row, at)? as u64,
        skills: db::int(row, at + 1)? as u64,
        user: db::int(row, at + 2)? as u64,
        tool: db::int(row, at + 3)? as u64,
        output: db::int(row, at + 4)? as u64,
        reasoning: db::int(row, at + 5)? as u64,
        unattributed: db::int(row, at + 6)? as u64,
    })
}

pub(crate) async fn record_turn_usage(
    database: &Database,
    row: &TurnUsageRow,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let t = &row.tokens;
    let a = &row.attribution;
    db::exec(
        &connection,
        "INSERT INTO turn_usage(
             thread_id, turn_id, harness, model,
             input_tokens, cached_input_tokens, cache_write_input_tokens,
             output_tokens, reasoning_output_tokens, cost_usd,
             context_tokens, context_window,
             attr_system, attr_skills, attr_user, attr_tool, attr_output,
             attr_reasoning, attr_unattributed, recorded_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(thread_id, turn_id) DO UPDATE SET
             harness = excluded.harness,
             model = excluded.model,
             input_tokens = excluded.input_tokens,
             cached_input_tokens = excluded.cached_input_tokens,
             cache_write_input_tokens = excluded.cache_write_input_tokens,
             output_tokens = excluded.output_tokens,
             reasoning_output_tokens = excluded.reasoning_output_tokens,
             cost_usd = excluded.cost_usd,
             context_tokens = excluded.context_tokens,
             context_window = excluded.context_window,
             attr_system = excluded.attr_system,
             attr_skills = excluded.attr_skills,
             attr_user = excluded.attr_user,
             attr_tool = excluded.attr_tool,
             attr_output = excluded.attr_output,
             attr_reasoning = excluded.attr_reasoning,
             attr_unattributed = excluded.attr_unattributed,
             recorded_at = excluded.recorded_at",
        params![
            row.thread_id.as_str(),
            row.turn_id.as_str(),
            row.harness.as_str(),
            row.model.as_deref(),
            t.input as i64,
            t.cached_input as i64,
            t.cache_write_input as i64,
            t.output as i64,
            t.reasoning_output as i64,
            row.cost_usd,
            row.context_tokens.map(|n| n as i64),
            row.context_window.map(|n| n as i64),
            a.system as i64,
            a.skills as i64,
            a.user as i64,
            a.tool as i64,
            a.output as i64,
            a.reasoning as i64,
            a.unattributed as i64,
            row.recorded_at,
        ],
    )
    .await
}

/// A scope's `WHERE` clause and its parameters, for composing the sums.
/// `col` prefixes the `turn_usage` columns (`"u."` when the table is aliased).
fn scope_filter(scope: &UsageScope, since: Option<i64>, col: &str) -> (String, Vec<turso::Value>) {
    let mut clauses = Vec::new();
    let mut values: Vec<turso::Value> = Vec::new();
    match scope {
        UsageScope::Thread(thread_id) => {
            clauses.push(format!("{col}thread_id = ?"));
            values.push(thread_id.clone().into());
        }
        UsageScope::Project(path) => {
            let prefix = db::escape_like(path);
            clauses.push(format!(
                "{col}thread_id IN (
                    SELECT thread_id FROM thread_summaries
                    WHERE cwd = ? OR cwd LIKE ? ESCAPE '\\'
                    UNION
                    SELECT thread_id FROM harness_threads
                    WHERE cwd = ? OR cwd LIKE ? ESCAPE '\\')"
            ));
            for _ in 0..2 {
                values.push(path.clone().into());
                values.push(format!("{prefix}/%").into());
            }
        }
        UsageScope::Global => {}
    }
    if let Some(since) = since {
        clauses.push(format!("{col}recorded_at >= ?"));
        values.push(since.into());
    }
    let sql = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    (sql, values)
}

pub(crate) async fn read_usage_totals(
    database: &Database,
    scope: &UsageScope,
    since: Option<i64>,
) -> Result<UsageTotals, String> {
    let connection = db::conn(database)?;
    let (filter, values) = scope_filter(scope, since, "");
    db::one(
        &connection,
        &format!(
            "SELECT {TOKEN_SUMS}, {ATTR_SUMS}, {COST_SUMS}, COUNT(*) FROM turn_usage {filter}"
        ),
        values,
        |row| {
            Ok(UsageTotals {
                tokens: tokens_at(row, 0)?,
                attribution: attribution_at(row, 5)?,
                cost_usd: db::opt_real(row, 12)?,
                unpriced: db::int(row, 13)? != 0,
                turns: db::int(row, 14)? as u64,
            })
        },
    )
    .await
    .map(Option::unwrap_or_default)
}

const ROW_COLUMNS: &str = "thread_id, turn_id, harness, model,
     input_tokens, cached_input_tokens, cache_write_input_tokens,
     output_tokens, reasoning_output_tokens, cost_usd,
     context_tokens, context_window,
     attr_system, attr_skills, attr_user, attr_tool, attr_output,
     attr_reasoning, attr_unattributed, recorded_at";

fn row_from(row: &turso::Row) -> Result<TurnUsageRow, String> {
    Ok(TurnUsageRow {
        thread_id: db::text(row, 0)?,
        turn_id: db::text(row, 1)?,
        harness: db::text(row, 2)?,
        model: db::opt_text(row, 3)?,
        tokens: tokens_at(row, 4)?,
        cost_usd: db::opt_real(row, 9)?,
        context_tokens: db::opt_int(row, 10)?.map(|n| n as u64),
        context_window: db::opt_int(row, 11)?.map(|n| n as u64),
        attribution: attribution_at(row, 12)?,
        recorded_at: db::int(row, 19)?,
    })
}

/// The most recently recorded row of a thread: what its context looked like
/// after the last turn this process, or an earlier one, saw.
pub(crate) async fn read_latest_turn_usage(
    database: &Database,
    thread_id: &str,
) -> Result<Option<TurnUsageRow>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        &format!(
            "SELECT {ROW_COLUMNS} FROM turn_usage
             WHERE thread_id = ? AND turn_id != '{OPENING_TURN_ID}'
             ORDER BY recorded_at DESC, rowid DESC LIMIT 1"
        ),
        (thread_id,),
        row_from,
    )
    .await
}

pub(crate) async fn read_turn_usage(
    database: &Database,
    thread_id: &str,
    turn_id: &str,
) -> Result<Option<TurnUsageRow>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        &format!(
            "SELECT {ROW_COLUMNS} FROM turn_usage WHERE thread_id = ? AND turn_id = ? LIMIT 1"
        ),
        (thread_id, turn_id),
        row_from,
    )
    .await
}

pub(crate) async fn read_usage_breakdown(
    database: &Database,
    scope: &UsageScope,
    since: Option<i64>,
) -> Result<StoredUsageBreakdown, String> {
    let totals = read_usage_totals(database, scope, since).await?;
    let connection = db::conn(database)?;
    let (filter, values) = scope_filter(scope, since, "");
    let by_model = db::rows(
        &connection,
        &format!(
            "SELECT model, harness, {TOKEN_SUMS}, SUM(cost_usd), COUNT(*)
             FROM turn_usage {filter}
             GROUP BY model, harness
             ORDER BY SUM(input_tokens + output_tokens) DESC"
        ),
        values.clone(),
        |row| {
            Ok(ModelUsageRow {
                model: db::opt_text(row, 0)?,
                harness: db::text(row, 1)?,
                tokens: tokens_at(row, 2)?,
                cost_usd: db::opt_real(row, 7)?,
                turns: db::int(row, 8)? as u64,
            })
        },
    )
    .await?;
    let by_thread = if matches!(scope, UsageScope::Thread(_)) {
        Vec::new()
    } else {
        let (thread_filter, mut thread_values) = scope_filter(scope, since, "u.");
        thread_values.push(BY_THREAD_LIMIT.into());
        db::rows(
            &connection,
            &format!(
                "SELECT u.thread_id, {}, {}, SUM(u.cost_usd), MAX(u.recorded_at),
                        COALESCE(s.title, h.title)
                 FROM turn_usage u
                 LEFT JOIN thread_summaries s ON s.thread_id = u.thread_id
                 LEFT JOIN harness_threads h ON h.thread_id = u.thread_id
                 {}
                 GROUP BY u.thread_id
                 ORDER BY SUM(u.input_tokens + u.output_tokens) DESC
                 LIMIT ?",
                TOKEN_SUMS.replace("SUM(", "SUM(u."),
                ATTR_SUMS.replace("SUM(", "SUM(u."),
                thread_filter
            ),
            thread_values,
            |row| {
                Ok(ThreadUsageRow {
                    thread_id: db::text(row, 0)?,
                    tokens: tokens_at(row, 1)?,
                    attribution: attribution_at(row, 6)?,
                    cost_usd: db::opt_real(row, 13)?,
                    last_at: db::int(row, 14)?,
                    title: db::opt_text(row, 15)?,
                })
            },
        )
        .await?
    };
    Ok(StoredUsageBreakdown {
        totals,
        by_model,
        by_thread,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> Database {
        let directory = tempfile::tempdir().unwrap();
        let database = crate::storage::open(directory.path()).await.unwrap();
        std::mem::forget(directory);
        database
    }

    fn row(thread: &str, turn: &str, input: u64, output: u64, at: i64) -> TurnUsageRow {
        TurnUsageRow {
            thread_id: thread.into(),
            turn_id: turn.into(),
            harness: "codex".into(),
            model: Some("gpt-5".into()),
            tokens: UsageTokens {
                input,
                cached_input: input / 2,
                cache_write_input: 0,
                output,
                reasoning_output: output / 4,
            },
            cost_usd: None,
            context_tokens: Some(input + output),
            context_window: Some(200_000),
            attribution: CategoryTokens {
                system: input / 2,
                user: input - input / 2,
                output: output - output / 4,
                reasoning: output / 4,
                ..Default::default()
            },
            recorded_at: at,
        }
    }

    async fn summary(database: &Database, thread: &str, cwd: &str) {
        let connection = db::conn(database).unwrap();
        db::exec(
            &connection,
            "INSERT INTO thread_summaries(thread_id, cwd, title, updated_at, status)
             VALUES (?, ?, ?, 0, 'idle')",
            params![thread, cwd, format!("Thread {thread}")],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_second_write_replaces_the_turn() {
        let database = database().await;
        record_turn_usage(&database, &row("t1", "turn-1", 100, 10, 1))
            .await
            .unwrap();
        record_turn_usage(&database, &row("t1", "turn-1", 300, 30, 2))
            .await
            .unwrap();
        let totals = read_usage_totals(&database, &UsageScope::Thread("t1".into()), None)
            .await
            .unwrap();
        assert_eq!(totals.turns, 1);
        assert_eq!(totals.tokens.input, 300);
        assert_eq!(totals.tokens.output, 30);
        assert_eq!(totals.attribution.total(), 330);
        assert!(totals.unpriced);
        assert_eq!(totals.cost_usd, None);
    }

    #[tokio::test]
    async fn reported_cost_is_summed_and_marks_nothing_unpriced() {
        let database = database().await;
        let mut first = row("t1", "turn-1", 100, 10, 1);
        first.cost_usd = Some(0.5);
        let mut second = row("t1", "turn-2", 100, 10, 2);
        second.cost_usd = Some(0.25);
        record_turn_usage(&database, &first).await.unwrap();
        record_turn_usage(&database, &second).await.unwrap();
        let totals = read_usage_totals(&database, &UsageScope::Thread("t1".into()), None)
            .await
            .unwrap();
        assert!((totals.cost_usd.unwrap() - 0.75).abs() < 1e-9);
        assert!(!totals.unpriced);
    }

    #[tokio::test]
    async fn a_project_sums_threads_under_its_path_across_both_thread_tables() {
        let database = database().await;
        summary(&database, "inside", "/repo").await;
        summary(&database, "nested", "/repo/sub").await;
        summary(&database, "lookalike", "/repo-other").await;
        crate::storage::record_harness_thread(&database, "claude", "claude", "/repo", "c", 0)
            .await
            .unwrap();
        for thread in ["inside", "nested", "lookalike", "claude", "orphan"] {
            record_turn_usage(&database, &row(thread, "turn-1", 100, 10, 1))
                .await
                .unwrap();
        }
        let project = read_usage_breakdown(&database, &UsageScope::Project("/repo".into()), None)
            .await
            .unwrap();
        assert_eq!(project.totals.turns, 3);
        assert_eq!(project.totals.tokens.input, 300);
        let mut listed: Vec<&str> = project
            .by_thread
            .iter()
            .map(|t| t.thread_id.as_str())
            .collect();
        listed.sort();
        assert_eq!(listed, ["claude", "inside", "nested"]);
        assert_eq!(
            project
                .by_thread
                .iter()
                .find(|t| t.thread_id == "inside")
                .unwrap()
                .title
                .as_deref(),
            Some("Thread inside")
        );
        let global = read_usage_breakdown(&database, &UsageScope::Global, None)
            .await
            .unwrap();
        assert_eq!(global.totals.turns, 5);
        assert_eq!(global.by_model.len(), 1);
        assert_eq!(global.by_model[0].turns, 5);
    }

    #[tokio::test]
    async fn since_drops_older_rows_and_threads_rank_by_tokens() {
        let database = database().await;
        record_turn_usage(&database, &row("small", "turn-1", 10, 1, 100))
            .await
            .unwrap();
        record_turn_usage(&database, &row("big", "turn-1", 1000, 100, 200))
            .await
            .unwrap();
        record_turn_usage(&database, &row("big", "turn-2", 1000, 100, 300))
            .await
            .unwrap();
        let all = read_usage_breakdown(&database, &UsageScope::Global, None)
            .await
            .unwrap();
        assert_eq!(all.by_thread[0].thread_id, "big");
        assert_eq!(all.by_thread[0].tokens.input, 2000);
        assert_eq!(all.by_thread[0].last_at, 300);
        let recent = read_usage_breakdown(&database, &UsageScope::Global, Some(250))
            .await
            .unwrap();
        assert_eq!(recent.totals.turns, 1);
        assert_eq!(recent.by_thread.len(), 1);
    }

    #[tokio::test]
    async fn the_latest_row_skips_the_opening_row() {
        let database = database().await;
        record_turn_usage(&database, &row("t1", OPENING_TURN_ID, 5000, 500, 900))
            .await
            .unwrap();
        assert!(read_latest_turn_usage(&database, "t1")
            .await
            .unwrap()
            .is_none());
        record_turn_usage(&database, &row("t1", "turn-1", 100, 10, 1))
            .await
            .unwrap();
        record_turn_usage(&database, &row("t1", "turn-2", 200, 20, 2))
            .await
            .unwrap();
        let latest = read_latest_turn_usage(&database, "t1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(latest.turn_id, "turn-2");
        assert_eq!(latest.context_tokens, Some(220));
    }
}
