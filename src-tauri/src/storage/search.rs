//! The local thread search index.
//!
//! Titles, previews, project paths and timestamps are mirrored here so history
//! search stays fast and works offline, while the canonical thread data lives in
//! the Codex app-server. Rows survive archiving (the flag flips) and are only
//! removed on a hard delete.

use serde::Serialize;
use turso::{params, Database};

use super::db;

/// The filter shared by the page query and its count, kept in one place so the
/// two can never drift and report a total that does not match the rows.
/// Parameters, in order: archived flag, project path (twice — the empty string
/// means "any project"), then the LIKE pattern three times.
const MATCH_CLAUSE: &str = "archived = ?
     AND (? = '' OR project_path = ?)
     AND (title LIKE ? ESCAPE '\\'
          OR preview LIKE ? ESCAPE '\\'
          OR project_path LIKE ? ESCAPE '\\')";

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredThreadSearch {
    pub(crate) thread_id: String,
    pub(crate) title: String,
    pub(crate) preview: String,
    pub(crate) project_path: String,
    pub(crate) updated_at: i64,
    pub(crate) archived: bool,
}

/// Insert or update a batch of search-index rows in one transaction.
pub(crate) async fn upsert_thread_search(
    database: &Database,
    rows: &[StoredThreadSearch],
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    for row in rows {
        db::exec(
            &transaction,
            "INSERT INTO thread_search(
                thread_id, title, preview, project_path, updated_at, archived
             ) VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(thread_id) DO UPDATE SET
                 title = excluded.title,
                 preview = excluded.preview,
                 project_path = excluded.project_path,
                 updated_at = excluded.updated_at,
                 archived = excluded.archived",
            params![
                row.thread_id.clone(),
                row.title.clone(),
                row.preview.clone(),
                row.project_path.clone(),
                row.updated_at,
                i64::from(row.archived)
            ],
        )
        .await?;
    }
    transaction.commit().await.map_err(db::db_error)
}

/// Update only the title of an indexed thread (e.g. after a rename).
pub(crate) async fn rename_thread_search(
    database: &Database,
    thread_id: &str,
    title: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE thread_search SET title = ? WHERE thread_id = ?",
        (title, thread_id),
    )
    .await
}

/// Flip the archived flag for an indexed thread. Archive/unarchive transitions
/// keep the row (with the flag updated) rather than deleting it.
pub(crate) async fn set_thread_search_archived(
    database: &Database,
    thread_id: &str,
    archived: bool,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE thread_search SET archived = ? WHERE thread_id = ?",
        (i64::from(archived), thread_id),
    )
    .await
}

/// Remove a thread from the search index (on hard delete).
pub(crate) async fn delete_thread_search(
    database: &Database,
    thread_id: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "DELETE FROM thread_search WHERE thread_id = ?",
        (thread_id,),
    )
    .await
}

/// Search the local index with a case-insensitive LIKE over title, preview and
/// project path. Returns a page of results ordered by recency plus the total
/// number of matches for the same filter (for "N of M" counts). An empty
/// `query` matches everything, which is handy for browsing within a filter.
pub(crate) async fn search_thread_index(
    database: &Database,
    query: &str,
    archived: bool,
    project_path: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<(Vec<StoredThreadSearch>, i64), String> {
    let connection = db::conn(database)?;
    let pattern = format!("%{}%", db::escape_like(query.trim()));
    let project = project_path.unwrap_or("").to_string();
    let archived = i64::from(archived);

    let total = db::one(
        &connection,
        &format!("SELECT COUNT(*) FROM thread_search WHERE {MATCH_CLAUSE}"),
        params![
            archived,
            project.clone(),
            project.clone(),
            pattern.clone(),
            pattern.clone(),
            pattern.clone()
        ],
        |row| db::int(row, 0),
    )
    .await?
    .unwrap_or(0);

    let results = db::rows(
        &connection,
        &format!(
            "SELECT thread_id, title, preview, project_path, updated_at, archived
             FROM thread_search
             WHERE {MATCH_CLAUSE}
             ORDER BY updated_at DESC, thread_id DESC
             LIMIT ? OFFSET ?"
        ),
        params![
            archived,
            project.clone(),
            project,
            pattern.clone(),
            pattern.clone(),
            pattern,
            limit,
            offset.max(0)
        ],
        |row| {
            Ok(StoredThreadSearch {
                thread_id: db::text(row, 0)?,
                title: db::text(row, 1)?,
                preview: db::text(row, 2)?,
                project_path: db::text(row, 3)?,
                updated_at: db::int(row, 4)?,
                archived: db::flag(row, 5)?,
            })
        },
    )
    .await?;
    Ok((results, total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    fn search_row(
        id: &str,
        title: &str,
        preview: &str,
        path: &str,
        updated: i64,
    ) -> StoredThreadSearch {
        StoredThreadSearch {
            thread_id: id.into(),
            title: title.into(),
            preview: preview.into(),
            project_path: path.into(),
            updated_at: updated,
            archived: false,
        }
    }

    #[tokio::test]
    async fn thread_search_round_trips_and_paginates_by_recency() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        upsert_thread_search(
            &database,
            &[
                search_row("a", "Alpha login", "fixing login", "/proj/one", 10),
                search_row("b", "Beta search", "add search bar", "/proj/one", 30),
                search_row("c", "Gamma report", "search index work", "/proj/two", 20),
            ],
        )
        .await
        .unwrap();

        // Matches title, preview and project path; ordered by updated_at desc.
        let (page, total) = search_thread_index(&database, "search", false, None, 0, 2)
            .await
            .unwrap();
        assert_eq!(total, 2);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].thread_id, "b");
        assert_eq!(page[1].thread_id, "c");

        // Cursor paging via offset.
        let (page2, total2) = search_thread_index(&database, "search", false, None, 2, 2)
            .await
            .unwrap();
        assert_eq!(total2, 2);
        assert!(page2.is_empty());

        // Project filter narrows the result set and its total.
        let (scoped, scoped_total) =
            search_thread_index(&database, "search", false, Some("/proj/two"), 0, 10)
                .await
                .unwrap();
        assert_eq!(scoped_total, 1);
        assert_eq!(scoped[0].thread_id, "c");
    }

    #[tokio::test]
    async fn thread_search_respects_archived_flag_and_like_escaping() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        upsert_thread_search(
            &database,
            &[
                search_row("a", "100% coverage", "raise it", "/proj", 5),
                search_row("b", "coverage plan", "todo", "/proj", 6),
            ],
        )
        .await
        .unwrap();

        // `%` is treated literally, so only the row containing it matches.
        let (page, total) = search_thread_index(&database, "100%", false, None, 0, 10)
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(page[0].thread_id, "a");

        // Archiving keeps the row but excludes it from active search.
        set_thread_search_archived(&database, "a", true)
            .await
            .unwrap();
        let (active, active_total) = search_thread_index(&database, "coverage", false, None, 0, 10)
            .await
            .unwrap();
        assert_eq!(active_total, 1);
        assert_eq!(active[0].thread_id, "b");
        let (archived, archived_total) =
            search_thread_index(&database, "coverage", true, None, 0, 10)
                .await
                .unwrap();
        assert_eq!(archived_total, 1);
        assert_eq!(archived[0].thread_id, "a");

        rename_thread_search(&database, "b", "renamed plan")
            .await
            .unwrap();
        let (renamed, _) = search_thread_index(&database, "renamed", false, None, 0, 10)
            .await
            .unwrap();
        assert_eq!(renamed.len(), 1);

        delete_thread_search(&database, "b").await.unwrap();
        let (gone, gone_total) = search_thread_index(&database, "renamed", false, None, 0, 10)
            .await
            .unwrap();
        assert!(gone.is_empty());
        assert_eq!(gone_total, 0);
    }
}
