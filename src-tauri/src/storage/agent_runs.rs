//! App-owned subagent runs.
//!
//! When the Pingex agent tools are on, a parent thread's `pingex_spawn_agent`
//! call starts a separate `codex app-server` process whose thread Codex does
//! *not* link back to the parent — `parentThreadId` is only set for its own
//! native subagents. So the parent/child relationship lives here, the same way
//! `side_questions` records a link Codex knows nothing about.
//!
//! Rows outlive the processes they describe: a run that was still `running`
//! when the app quit is reconciled to `orphaned` on the next launch, so the GUI
//! never shows a spinner that can never resolve.

use serde::Serialize;
use turso::{params, Database};

use super::db;

/// Terminal and non-terminal states a run can be in. Stored as text so an
/// unknown value from a future version degrades to "shown as-is" rather than
/// failing the read.
pub(crate) const STATUS_RUNNING: &str = "running";
pub(crate) const STATUS_DONE: &str = "done";
pub(crate) const STATUS_FAILED: &str = "failed";
pub(crate) const STATUS_KILLED: &str = "killed";
pub(crate) const STATUS_ORPHANED: &str = "orphaned";

/// One spawned agent, as the GUI sees it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRunRow {
    pub(crate) run_id: String,
    pub(crate) parent_thread_id: String,
    pub(crate) parent_turn_id: String,
    pub(crate) call_id: Option<String>,
    pub(crate) child_thread_id: Option<String>,
    pub(crate) name: String,
    pub(crate) prompt: String,
    pub(crate) cwd: String,
    pub(crate) model: Option<String>,
    pub(crate) reasoning_effort: Option<String>,
    pub(crate) status: String,
    pub(crate) result: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) created_at: i64,
    pub(crate) finished_at: Option<i64>,
}

pub(crate) async fn record_agent_run(database: &Database, run: &AgentRunRow) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO agent_runs(
             run_id, parent_thread_id, parent_turn_id, call_id, child_thread_id, name, prompt,
             cwd, model, reasoning_effort, status, result, error, created_at, finished_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(run_id) DO UPDATE SET
             child_thread_id = excluded.child_thread_id,
             status = excluded.status,
             result = excluded.result,
             error = excluded.error,
             finished_at = excluded.finished_at",
        params![
            run.run_id.as_str(),
            run.parent_thread_id.as_str(),
            run.parent_turn_id.as_str(),
            run.call_id.as_deref(),
            run.child_thread_id.as_deref(),
            run.name.as_str(),
            run.prompt.as_str(),
            run.cwd.as_str(),
            run.model.as_deref(),
            run.reasoning_effort.as_deref(),
            run.status.as_str(),
            run.result.as_deref(),
            run.error.as_deref(),
            run.created_at,
            run.finished_at
        ],
    )
    .await
}

/// Update the parts of a run that change as it progresses. Every field is
/// optional so a caller that only learned the child thread id does not have to
/// restate the status, and vice versa.
pub(crate) async fn update_agent_run(
    database: &Database,
    run_id: &str,
    status: Option<&str>,
    child_thread_id: Option<&str>,
    result: Option<&str>,
    error: Option<&str>,
    finished_at: Option<i64>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE agent_runs SET
             status = COALESCE(?, status),
             child_thread_id = COALESCE(?, child_thread_id),
             result = COALESCE(?, result),
             error = COALESCE(?, error),
             finished_at = COALESCE(?, finished_at)
         WHERE run_id = ?",
        params![status, child_thread_id, result, error, finished_at, run_id],
    )
    .await
}

const COLUMNS: &str = "run_id, parent_thread_id, parent_turn_id, call_id, child_thread_id, name,
     prompt, cwd, model, reasoning_effort, status, result, error, created_at, finished_at";

fn row_to_run(row: &turso::Row) -> Result<AgentRunRow, String> {
    Ok(AgentRunRow {
        run_id: db::text(row, 0)?,
        parent_thread_id: db::text(row, 1)?,
        parent_turn_id: db::text(row, 2)?,
        call_id: db::opt_text(row, 3)?,
        child_thread_id: db::opt_text(row, 4)?,
        name: db::text(row, 5)?,
        prompt: db::text(row, 6)?,
        cwd: db::text(row, 7)?,
        model: db::opt_text(row, 8)?,
        reasoning_effort: db::opt_text(row, 9)?,
        status: db::text(row, 10)?,
        result: db::opt_text(row, 11)?,
        error: db::opt_text(row, 12)?,
        created_at: db::int(row, 13)?,
        finished_at: db::opt_int(row, 14)?,
    })
}

pub(crate) async fn read_agent_runs(
    database: &Database,
    parent_thread_id: &str,
) -> Result<Vec<AgentRunRow>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {COLUMNS} FROM agent_runs WHERE parent_thread_id = ? ORDER BY created_at"
        ),
        (parent_thread_id,),
        row_to_run,
    )
    .await
}

pub(crate) async fn read_agent_run(
    database: &Database,
    run_id: &str,
) -> Result<Option<AgentRunRow>, String> {
    let connection = db::conn(database)?;
    let runs = db::rows(
        &connection,
        &format!("SELECT {COLUMNS} FROM agent_runs WHERE run_id = ?"),
        (run_id,),
        row_to_run,
    )
    .await?;
    Ok(runs.into_iter().next())
}

/// Every thread that exists only because it is somebody's subagent, paired with
/// the thread that spawned it. The sidebar hides the children so they do not
/// appear as top-level threads, and counts them against their parent.
pub(crate) async fn read_agent_run_children(
    database: &Database,
) -> Result<Vec<(String, String)>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT child_thread_id, parent_thread_id FROM agent_runs
         WHERE child_thread_id IS NOT NULL",
        (),
        |row| Ok((db::text(row, 0)?, db::text(row, 1)?)),
    )
    .await
}

pub(crate) async fn delete_agent_runs(
    database: &Database,
    parent_thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM agent_runs WHERE parent_thread_id = ?",
        (parent_thread_id,),
    )
    .await
}

/// Carry runs over to a forked thread, which inherits the turns that spawned
/// them. The copies get fresh run ids so the two threads cannot fight over one
/// row, but they point at the same child threads — the transcript is shared,
/// and neither fork owns the process (which is long gone by fork time).
pub(crate) async fn copy_agent_runs(
    database: &Database,
    from_thread_id: &str,
    to_thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO agent_runs(
             run_id, parent_thread_id, parent_turn_id, call_id, child_thread_id, name, prompt,
             cwd, model, reasoning_effort, status, result, error, created_at, finished_at)
         SELECT ? || run_id, ?, parent_turn_id, call_id, child_thread_id, name, prompt,
             cwd, model, reasoning_effort, status, result, error, created_at, finished_at
         FROM agent_runs WHERE parent_thread_id = ?
         ON CONFLICT(run_id) DO NOTHING",
        (
            format!("{to_thread_id}:"),
            to_thread_id.to_string(),
            from_thread_id.to_string(),
        ),
    )
    .await
}

/// Drop runs belonging to turns a rollback removed.
pub(crate) async fn retain_agent_runs(
    database: &Database,
    parent_thread_id: &str,
    keep: &[String],
) -> Result<(), String> {
    if keep.is_empty() {
        return delete_agent_runs(database, parent_thread_id).await;
    }
    let connection = db::conn(database)?;
    // Turn ids come from Codex (UUIDs); they are quoted here because turso's
    // parameter binding cannot expand a list.
    let list = keep
        .iter()
        .map(|id| format!("'{}'", id.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    db::exec(
        &connection,
        &format!(
            "DELETE FROM agent_runs
             WHERE parent_thread_id = ? AND parent_turn_id NOT IN ({list})"
        ),
        (parent_thread_id,),
    )
    .await
}

/// Reconcile runs left mid-flight by a crash or a quit. Their processes died
/// with the app, so nothing will ever complete them.
pub(crate) async fn orphan_running_agent_runs(database: &Database) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE agent_runs SET status = ?, error = COALESCE(error, ?)
         WHERE status = ?",
        params![
            STATUS_ORPHANED,
            "The app exited while this agent was running.",
            STATUS_RUNNING
        ],
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn database() -> Database {
        let directory = tempfile::tempdir().unwrap();
        let database = crate::storage::open(directory.path()).await.unwrap();
        // The temp directory only has to outlive `open`; the connection keeps
        // the file alive for the rest of the test.
        std::mem::forget(directory);
        database
    }

    fn run(run_id: &str, thread_id: &str, turn_id: &str) -> AgentRunRow {
        AgentRunRow {
            run_id: run_id.into(),
            parent_thread_id: thread_id.into(),
            parent_turn_id: turn_id.into(),
            call_id: Some("call-1".into()),
            child_thread_id: None,
            name: "probe".into(),
            prompt: "say hi".into(),
            cwd: "/tmp".into(),
            model: Some("gpt-5.2".into()),
            reasoning_effort: None,
            status: STATUS_RUNNING.into(),
            result: None,
            error: None,
            created_at: 100,
            finished_at: None,
        }
    }

    #[tokio::test]
    async fn records_a_run_and_reads_it_back() {
        let database = database().await;
        record_agent_run(&database, &run("run-1", "thread-1", "turn-1"))
            .await
            .unwrap();

        let runs = read_agent_runs(&database, "thread-1").await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].name, "probe");
        assert_eq!(runs[0].status, STATUS_RUNNING);
        assert_eq!(runs[0].child_thread_id, None);
    }

    #[tokio::test]
    async fn updates_only_the_fields_it_is_given() {
        let database = database().await;
        record_agent_run(&database, &run("run-1", "thread-1", "turn-1"))
            .await
            .unwrap();

        update_agent_run(&database, "run-1", None, Some("child-1"), None, None, None)
            .await
            .unwrap();
        update_agent_run(
            &database,
            "run-1",
            Some(STATUS_DONE),
            None,
            Some("all done"),
            None,
            Some(200),
        )
        .await
        .unwrap();

        let stored = read_agent_run(&database, "run-1").await.unwrap().unwrap();
        assert_eq!(stored.child_thread_id.as_deref(), Some("child-1"));
        assert_eq!(stored.status, STATUS_DONE);
        assert_eq!(stored.result.as_deref(), Some("all done"));
        assert_eq!(stored.finished_at, Some(200));
        // Untouched by both updates.
        assert_eq!(stored.prompt, "say hi");
    }

    #[tokio::test]
    async fn lists_the_child_threads_the_sidebar_should_hide() {
        let database = database().await;
        record_agent_run(&database, &run("run-1", "thread-1", "turn-1"))
            .await
            .unwrap();
        record_agent_run(&database, &run("run-2", "thread-1", "turn-1"))
            .await
            .unwrap();
        update_agent_run(&database, "run-2", None, Some("child-2"), None, None, None)
            .await
            .unwrap();

        // Only the run that got as far as starting a thread contributes.
        assert_eq!(
            read_agent_run_children(&database).await.unwrap(),
            vec![("child-2".to_string(), "thread-1".to_string())]
        );
    }

    #[tokio::test]
    async fn a_fork_inherits_the_runs_of_the_thread_it_came_from() {
        let database = database().await;
        record_agent_run(&database, &run("run-1", "thread-1", "turn-1"))
            .await
            .unwrap();

        copy_agent_runs(&database, "thread-1", "fork-1").await.unwrap();

        let forked = read_agent_runs(&database, "fork-1").await.unwrap();
        assert_eq!(forked.len(), 1);
        assert_eq!(forked[0].name, "probe");
        // Fresh id, so updating one fork cannot disturb the other.
        assert_ne!(forked[0].run_id, "run-1");
        assert_eq!(read_agent_runs(&database, "thread-1").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn retains_only_the_runs_whose_turns_survived_a_rollback() {
        let database = database().await;
        record_agent_run(&database, &run("run-1", "thread-1", "turn-1"))
            .await
            .unwrap();
        record_agent_run(&database, &run("run-2", "thread-1", "turn-2"))
            .await
            .unwrap();

        retain_agent_runs(&database, "thread-1", &["turn-1".to_string()])
            .await
            .unwrap();
        let runs = read_agent_runs(&database, "thread-1").await.unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].parent_turn_id, "turn-1");

        retain_agent_runs(&database, "thread-1", &[]).await.unwrap();
        assert!(read_agent_runs(&database, "thread-1")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn deleting_a_thread_takes_its_runs_with_it() {
        let database = database().await;
        record_agent_run(&database, &run("run-1", "thread-1", "turn-1"))
            .await
            .unwrap();
        record_agent_run(&database, &run("run-2", "thread-2", "turn-1"))
            .await
            .unwrap();

        delete_agent_runs(&database, "thread-1").await.unwrap();
        assert!(read_agent_runs(&database, "thread-1")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(read_agent_runs(&database, "thread-2").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_run_left_mid_flight_by_a_quit_is_reconciled_on_the_next_launch() {
        let database = database().await;
        record_agent_run(&database, &run("run-1", "thread-1", "turn-1"))
            .await
            .unwrap();
        let mut finished = run("run-2", "thread-1", "turn-1");
        finished.status = STATUS_DONE.into();
        finished.result = Some("kept".into());
        record_agent_run(&database, &finished).await.unwrap();

        orphan_running_agent_runs(&database).await.unwrap();

        let runs = read_agent_runs(&database, "thread-1").await.unwrap();
        let orphaned = runs.iter().find(|run| run.run_id == "run-1").unwrap();
        assert_eq!(orphaned.status, STATUS_ORPHANED);
        assert!(orphaned.error.is_some());
        // A run that had already finished is left alone.
        let done = runs.iter().find(|run| run.run_id == "run-2").unwrap();
        assert_eq!(done.status, STATUS_DONE);
        assert_eq!(done.result.as_deref(), Some("kept"));
    }
}
