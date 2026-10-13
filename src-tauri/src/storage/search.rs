//! The local thread search index.
//!
//! Titles, previews, project paths and timestamps are mirrored here so history
//! search stays fast and works offline, while the canonical thread data lives in
//! the Codex app-server. Rows survive archiving (the flag flips) and are only
//! removed on a hard delete.

use serde::Serialize;
use turso::{params, Database};

use super::db;

/// Which journaled message bodies a search should also scan. Both default to
/// off, so plain searches stay a cheap scan over the small `thread_search`
/// table; each enabled role adds a subquery over `thread_items`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MessageScope {
    pub(crate) agent: bool,
    pub(crate) user: bool,
}

impl MessageScope {
    pub(crate) fn any(self) -> bool {
        self.agent || self.user
    }
}

/// Subquery selecting the ids of threads with a journaled item of `kind`
/// whose message text matches the LIKE pattern bound in its place. The
/// pattern runs over the decoded text, not the serialized payload, so JSON
/// keys never match and quotes or backslashes match as typed. Payloads are
/// compact `serde_json` strings, so the `"type":"…"` prefilter is literal.
fn message_subquery(kind: &str) -> String {
    let prefilter = format!("payload LIKE '%\"type\":\"{kind}\"%'");
    if kind == "userMessage" {
        // A user message's text is spread over its `content` parts.
        format!(
            "SELECT DISTINCT thread_items.thread_id
             FROM thread_items, json_each(thread_items.payload, '$.content') AS part
             WHERE {prefilter}
               AND json_extract(part.value, '$.text') LIKE ? ESCAPE '\\'"
        )
    } else {
        format!(
            "SELECT DISTINCT thread_id FROM thread_items
             WHERE {prefilter}
               AND json_extract(payload, '$.text') LIKE ? ESCAPE '\\'"
        )
    }
}

/// The filter shared by the page query and its count, built in one place so the
/// two can never drift and report a total that does not match the rows.
/// Parameters, in order: archived flag, project path (twice — the empty string
/// means "any project"), the LIKE pattern three times, then once more per
/// enabled message role.
fn match_clause(scope: MessageScope) -> String {
    let mut clause = String::from(
        "archived = ?
     AND (? = '' OR project_path = ?)
     AND (title LIKE ? ESCAPE '\\'
          OR preview LIKE ? ESCAPE '\\'
          OR project_path LIKE ? ESCAPE '\\'",
    );
    if scope.agent {
        clause.push_str(&format!(
            "\n          OR thread_id IN ({})",
            message_subquery("agentMessage")
        ));
    }
    if scope.user {
        clause.push_str(&format!(
            "\n          OR thread_id IN ({})",
            message_subquery("userMessage")
        ));
    }
    clause.push(')');
    clause
}

fn match_params(
    archived: i64,
    project: &str,
    pattern: &str,
    scope: MessageScope,
) -> Vec<turso::Value> {
    let mut values = vec![
        turso::Value::Integer(archived),
        turso::Value::Text(project.to_string()),
        turso::Value::Text(project.to_string()),
        turso::Value::Text(pattern.to_string()),
        turso::Value::Text(pattern.to_string()),
        turso::Value::Text(pattern.to_string()),
    ];
    if scope.agent {
        values.push(turso::Value::Text(pattern.to_string()));
    }
    if scope.user {
        values.push(turso::Value::Text(pattern.to_string()));
    }
    values
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, specta::Type)]
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
    scope: MessageScope,
    offset: i64,
    limit: i64,
) -> Result<(Vec<StoredThreadSearch>, i64), String> {
    let connection = db::conn(database)?;
    let pattern = format!("%{}%", db::escape_like(query.trim()));
    let project = project_path.unwrap_or("");
    let archived = i64::from(archived);
    let clause = match_clause(scope);

    let total = db::one(
        &connection,
        &format!("SELECT COUNT(*) FROM thread_search WHERE {clause}"),
        match_params(archived, project, &pattern, scope),
        |row| db::int(row, 0),
    )
    .await?
    .unwrap_or(0);

    let mut page_params = match_params(archived, project, &pattern, scope);
    page_params.push(turso::Value::Integer(limit));
    page_params.push(turso::Value::Integer(offset.max(0)));
    let results = db::rows(
        &connection,
        &format!(
            "SELECT thread_id, title, preview, project_path, updated_at, archived
             FROM thread_search
             WHERE {clause}
             ORDER BY updated_at DESC, thread_id DESC
             LIMIT ? OFFSET ?"
        ),
        page_params,
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

/// Ids of threads with a journaled message of an enabled role whose body
/// matches `query`. Used by harnesses whose thread list is not in the
/// `thread_search` index and is filtered in memory instead.
pub(crate) async fn thread_ids_with_message_match(
    database: &Database,
    query: &str,
    scope: MessageScope,
) -> Result<Vec<String>, String> {
    if !scope.any() {
        return Ok(Vec::new());
    }
    let connection = db::conn(database)?;
    let pattern = format!("%{}%", db::escape_like(query.trim()));
    let mut ids = Vec::new();
    for (enabled, kind) in [(scope.agent, "agentMessage"), (scope.user, "userMessage")] {
        if !enabled {
            continue;
        }
        let found = db::rows(
            &connection,
            &message_subquery(kind),
            params![pattern.clone()],
            |row| db::text(row, 0),
        )
        .await?;
        ids.extend(found);
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
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
        let (page, total) = search_thread_index(
            &database,
            "search",
            false,
            None,
            MessageScope::default(),
            0,
            2,
        )
        .await
        .unwrap();
        assert_eq!(total, 2);
        assert_eq!(page.len(), 2);
        assert_eq!(page[0].thread_id, "b");
        assert_eq!(page[1].thread_id, "c");

        // Cursor paging via offset.
        let (page2, total2) = search_thread_index(
            &database,
            "search",
            false,
            None,
            MessageScope::default(),
            2,
            2,
        )
        .await
        .unwrap();
        assert_eq!(total2, 2);
        assert!(page2.is_empty());

        // Project filter narrows the result set and its total.
        let (scoped, scoped_total) = search_thread_index(
            &database,
            "search",
            false,
            Some("/proj/two"),
            MessageScope::default(),
            0,
            10,
        )
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
        let (page, total) = search_thread_index(
            &database,
            "100%",
            false,
            None,
            MessageScope::default(),
            0,
            10,
        )
        .await
        .unwrap();
        assert_eq!(total, 1);
        assert_eq!(page[0].thread_id, "a");

        // Archiving keeps the row but excludes it from active search.
        set_thread_search_archived(&database, "a", true)
            .await
            .unwrap();
        let (active, active_total) = search_thread_index(
            &database,
            "coverage",
            false,
            None,
            MessageScope::default(),
            0,
            10,
        )
        .await
        .unwrap();
        assert_eq!(active_total, 1);
        assert_eq!(active[0].thread_id, "b");
        let (archived, archived_total) = search_thread_index(
            &database,
            "coverage",
            true,
            None,
            MessageScope::default(),
            0,
            10,
        )
        .await
        .unwrap();
        assert_eq!(archived_total, 1);
        assert_eq!(archived[0].thread_id, "a");

        rename_thread_search(&database, "b", "renamed plan")
            .await
            .unwrap();
        let (renamed, _) = search_thread_index(
            &database,
            "renamed",
            false,
            None,
            MessageScope::default(),
            0,
            10,
        )
        .await
        .unwrap();
        assert_eq!(renamed.len(), 1);

        delete_thread_search(&database, "b").await.unwrap();
        let (gone, gone_total) = search_thread_index(
            &database,
            "renamed",
            false,
            None,
            MessageScope::default(),
            0,
            10,
        )
        .await
        .unwrap();
        assert!(gone.is_empty());
        assert_eq!(gone_total, 0);
    }

    #[tokio::test]
    async fn thread_search_scans_journaled_message_bodies_per_role() {
        use crate::storage::record_thread_item;
        use serde_json::json;

        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        upsert_thread_search(
            &database,
            &[
                search_row("a", "Alpha", "first", "/proj", 10),
                search_row("b", "Beta", "second", "/proj", 20),
            ],
        )
        .await
        .unwrap();
        record_thread_item(
            &database,
            "a",
            "turn",
            "u1",
            &json!({"type": "userMessage", "content": [{"type": "text", "text": "please add pagination"}]}),
            None,
        )
        .await
        .unwrap();
        record_thread_item(
            &database,
            "b",
            "turn",
            "m1",
            &json!({"type": "agentMessage", "text": "Implemented the cursor logic."}),
            None,
        )
        .await
        .unwrap();

        let run = |query: &'static str, scope: MessageScope| {
            let database = &database;
            async move {
                let (rows, total) = search_thread_index(database, query, false, None, scope, 0, 10)
                    .await
                    .unwrap();
                let ids: Vec<_> = rows.into_iter().map(|row| row.thread_id).collect();
                assert_eq!(total, ids.len() as i64, "count and page must agree");
                ids
            }
        };

        // Off by default: bodies are invisible.
        assert!(run("pagination", MessageScope::default()).await.is_empty());
        assert!(run("cursor", MessageScope::default()).await.is_empty());

        let user = MessageScope {
            user: true,
            agent: false,
        };
        let agent = MessageScope {
            user: false,
            agent: true,
        };
        assert_eq!(run("pagination", user).await, vec!["a"]);
        assert!(run("pagination", agent).await.is_empty());
        assert_eq!(run("cursor", agent).await, vec!["b"]);
        assert!(run("cursor", user).await.is_empty());

        // Index matches still apply alongside body matches, newest first.
        let both = MessageScope {
            user: true,
            agent: true,
        };
        assert_eq!(run("cursor", both).await, vec!["b"]);
        assert_eq!(run("Alpha", both).await, vec!["a"]);

        // The text matches, not the JSON around it.
        assert!(run("type", both).await.is_empty());
        assert!(run("text", both).await.is_empty());
        record_thread_item(
            &database,
            "b",
            "turn",
            "m2",
            &json!({"type": "agentMessage", "text": r#"Set "mode" in C:\temp"#}),
            None,
        )
        .await
        .unwrap();
        record_thread_item(
            &database,
            "a",
            "turn",
            "u2",
            &json!({"type": "userMessage", "content": [{"type": "text", "text": r#"why "quoted""#}]}),
            None,
        )
        .await
        .unwrap();
        assert_eq!(run(r#""mode""#, agent).await, vec!["b"]);
        assert_eq!(run(r#"C:\temp"#, agent).await, vec!["b"]);
        assert_eq!(run(r#"why "quoted""#, user).await, vec!["a"]);

        let ids = thread_ids_with_message_match(&database, "cursor", both)
            .await
            .unwrap();
        assert_eq!(ids, vec!["b"]);
        assert!(
            thread_ids_with_message_match(&database, "cursor", MessageScope::default())
                .await
                .unwrap()
                .is_empty()
        );
    }
}
