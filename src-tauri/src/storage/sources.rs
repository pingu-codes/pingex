//! Project instructions and the attached sources that feed workspace search.
//!
//! A source is a folder or single file the user attaches to a project; its text
//! content is flattened into `index_lines`, one row per non-blank line, which a
//! LIKE query searches. The index is rebuilt wholesale per source, never patched.

use serde::Serialize;
use std::path::Path;
use turso::{params, Database};

use super::db;
use crate::util::time::unix_secs;

const SOURCE_COLUMNS: &str =
    "id, project_path, source_path, kind, added_at, status, indexed_at, doc_count, error";

/// One attached source (a folder or a single file) that contributes its text
/// content to a project's searchable index.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct StoredProjectSource {
    pub(crate) id: String,
    pub(crate) project_path: String,
    pub(crate) source_path: String,
    /// "folder" | "file".
    pub(crate) kind: String,
    pub(crate) added_at: i64,
    /// "pending" | "indexed" | "error".
    pub(crate) status: String,
    pub(crate) indexed_at: Option<i64>,
    pub(crate) doc_count: i64,
    pub(crate) error: Option<String>,
}

/// One indexed line of file content, the unit the LIKE-based workspace search
/// matches against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IndexedLine {
    pub(crate) file_path: String,
    pub(crate) file_name: String,
    pub(crate) line_number: i64,
    pub(crate) content: String,
}

/// One content-line hit from the project index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContentHit {
    pub(crate) file_path: String,
    pub(crate) file_name: String,
    pub(crate) line_number: i64,
    pub(crate) content: String,
}

fn source_from_row(row: &turso::Row) -> Result<StoredProjectSource, String> {
    Ok(StoredProjectSource {
        id: db::text(row, 0)?,
        project_path: db::text(row, 1)?,
        source_path: db::text(row, 2)?,
        kind: db::text(row, 3)?,
        added_at: db::int(row, 4)?,
        status: db::text(row, 5)?,
        indexed_at: db::opt_int(row, 6)?,
        doc_count: db::int(row, 7)?,
        error: db::opt_text(row, 8)?,
    })
}

// --- Project instructions ---------------------------------------------------

/// Read the instructions of the project whose path is the longest prefix of
/// `cwd` — so a turn started anywhere inside a project (or its worktree) picks
/// up that project's instructions.
pub(crate) async fn read_instructions_for_cwd(
    database: &Database,
    cwd: &str,
) -> Result<Option<String>, String> {
    let mut best: Option<(usize, String)> = None;
    for (project_path, instructions) in read_all_project_instructions(database).await? {
        if instructions.trim().is_empty() {
            continue;
        }
        if Path::new(cwd).starts_with(Path::new(&project_path)) {
            let len = project_path.len();
            if best.as_ref().is_none_or(|(best_len, _)| len > *best_len) {
                best = Some((len, instructions));
            }
        }
    }
    Ok(best.map(|(_, instructions)| instructions))
}

pub(crate) async fn read_all_project_instructions(
    database: &Database,
) -> Result<Vec<(String, String)>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        "SELECT project_path, instructions FROM project_instructions",
        (),
        |row| Ok((db::text(row, 0)?, db::text(row, 1)?)),
    )
    .await
}

/// Save a project's instructions. Blank instructions clear the row rather than
/// storing an empty string, so "has instructions" is a simple row check.
pub(crate) async fn write_project_instructions(
    database: &Database,
    project_path: &str,
    instructions: &str,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    if instructions.trim().is_empty() {
        return db::exec(
            &connection,
            "DELETE FROM project_instructions WHERE project_path = ?",
            (project_path,),
        )
        .await;
    }
    db::exec(
        &connection,
        "INSERT INTO project_instructions(project_path, instructions, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(project_path) DO UPDATE SET
             instructions = excluded.instructions,
             updated_at = excluded.updated_at",
        params![project_path, instructions, unix_secs()],
    )
    .await
}

// --- Attached sources -------------------------------------------------------

pub(crate) async fn read_all_project_sources(
    database: &Database,
) -> Result<Vec<StoredProjectSource>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!("SELECT {SOURCE_COLUMNS} FROM project_sources ORDER BY added_at"),
        (),
        source_from_row,
    )
    .await
}

pub(crate) async fn read_project_sources(
    database: &Database,
    project_path: &str,
) -> Result<Vec<StoredProjectSource>, String> {
    let connection = db::conn(database)?;
    db::rows(
        &connection,
        &format!(
            "SELECT {SOURCE_COLUMNS} FROM project_sources
             WHERE project_path = ? ORDER BY added_at"
        ),
        (project_path,),
        source_from_row,
    )
    .await
}

pub(crate) async fn read_source(
    database: &Database,
    id: &str,
) -> Result<Option<StoredProjectSource>, String> {
    let connection = db::conn(database)?;
    db::one(
        &connection,
        &format!("SELECT {SOURCE_COLUMNS} FROM project_sources WHERE id = ?"),
        (id,),
        source_from_row,
    )
    .await
}

pub(crate) async fn insert_project_source(
    database: &Database,
    source: &StoredProjectSource,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "INSERT INTO project_sources(
            id, project_path, source_path, kind, added_at, status, indexed_at, doc_count, error
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        params![
            source.id.clone(),
            source.project_path.clone(),
            source.source_path.clone(),
            source.kind.clone(),
            source.added_at,
            source.status.clone(),
            source.indexed_at,
            source.doc_count,
            source.error.clone()
        ],
    )
    .await
}

pub(crate) async fn set_source_status(
    database: &Database,
    id: &str,
    status: &str,
    indexed_at: Option<i64>,
    doc_count: i64,
    error: Option<&str>,
) -> Result<(), String> {
    let connection = db::conn(database)?;
    db::exec(
        &connection,
        "UPDATE project_sources
         SET status = ?, indexed_at = ?, doc_count = ?, error = ?
         WHERE id = ?",
        params![status, indexed_at, doc_count, error, id],
    )
    .await
}

/// Remove a source and everything it contributed to the index.
pub(crate) async fn delete_project_source(database: &Database, id: &str) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(
        &transaction,
        "DELETE FROM index_lines WHERE source_id = ?",
        (id,),
    )
    .await?;
    db::exec(
        &transaction,
        "DELETE FROM project_sources WHERE id = ?",
        (id,),
    )
    .await?;
    transaction.commit().await.map_err(db::db_error)
}

// --- Content index ----------------------------------------------------------

/// Replace the indexed content of one source with `lines` in a single
/// transaction, so a reindex never leaves a half-written index behind.
pub(crate) async fn replace_source_lines(
    database: &Database,
    source_id: &str,
    project_path: &str,
    lines: &[IndexedLine],
) -> Result<(), String> {
    let connection = db::conn(database)?;
    let transaction = connection
        .unchecked_transaction()
        .await
        .map_err(db::db_error)?;
    db::exec(
        &transaction,
        "DELETE FROM index_lines WHERE source_id = ?",
        (source_id,),
    )
    .await?;
    for line in lines {
        db::exec(
            &transaction,
            "INSERT INTO index_lines(
                source_id, project_path, file_path, file_name, line_number, content
             ) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                source_id,
                project_path,
                line.file_path.clone(),
                line.file_name.clone(),
                line.line_number,
                line.content.clone()
            ],
        )
        .await?;
    }
    transaction.commit().await.map_err(db::db_error)
}

/// LIKE-based content search over a project's indexed lines. Returns one hit
/// per matching line, ordered by file then line, windowed by `offset`/`limit`
/// so the caller can page with a cursor.
pub(crate) async fn search_index_lines(
    database: &Database,
    project_path: &str,
    query: &str,
    limit: usize,
    offset: usize,
) -> Result<Vec<ContentHit>, String> {
    let connection = db::conn(database)?;
    let pattern = format!("%{}%", db::escape_like(query));
    db::rows(
        &connection,
        "SELECT file_path, file_name, line_number, content FROM index_lines
         WHERE project_path = ? AND content LIKE ? ESCAPE '\\'
         ORDER BY file_path, line_number
         LIMIT ? OFFSET ?",
        params![project_path, pattern, (limit + 1) as i64, offset as i64],
        |row| {
            Ok(ContentHit {
                file_path: db::text(row, 0)?,
                file_name: db::text(row, 1)?,
                line_number: db::int(row, 2)?,
                content: db::text(row, 3)?,
            })
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open;

    #[tokio::test]
    async fn round_trips_project_instructions_and_prefix_lookup() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        assert!(read_all_project_instructions(&database)
            .await
            .unwrap()
            .is_empty());

        write_project_instructions(&database, "/proj", "Use tabs.")
            .await
            .unwrap();
        assert_eq!(
            read_all_project_instructions(&database).await.unwrap(),
            vec![("/proj".to_string(), "Use tabs.".to_string())]
        );
        // A cwd inside the project (e.g. a worktree subdir) resolves to it.
        assert_eq!(
            read_instructions_for_cwd(&database, "/proj/src/lib")
                .await
                .unwrap(),
            Some("Use tabs.".to_string())
        );
        assert!(read_instructions_for_cwd(&database, "/other")
            .await
            .unwrap()
            .is_none());

        // Writing blank instructions clears the row.
        write_project_instructions(&database, "/proj", "   ")
            .await
            .unwrap();
        assert!(read_all_project_instructions(&database)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn the_longest_matching_project_prefix_wins() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        write_project_instructions(&database, "/proj", "outer")
            .await
            .unwrap();
        write_project_instructions(&database, "/proj/inner", "inner")
            .await
            .unwrap();
        assert_eq!(
            read_instructions_for_cwd(&database, "/proj/inner/src")
                .await
                .unwrap(),
            Some("inner".to_string())
        );
    }

    #[tokio::test]
    async fn round_trips_sources_and_content_search() {
        let directory = tempfile::tempdir().unwrap();
        let database = open(directory.path()).await.unwrap();
        let source = StoredProjectSource {
            id: "src-1".into(),
            project_path: "/proj".into(),
            source_path: "/proj".into(),
            kind: "folder".into(),
            added_at: 10,
            status: "pending".into(),
            indexed_at: None,
            doc_count: 0,
            error: None,
        };
        insert_project_source(&database, &source).await.unwrap();
        assert_eq!(
            read_project_sources(&database, "/proj").await.unwrap(),
            vec![source.clone()]
        );
        assert_eq!(read_all_project_sources(&database).await.unwrap().len(), 1);

        let lines = vec![
            IndexedLine {
                file_path: "src/main.rs".into(),
                file_name: "main.rs".into(),
                line_number: 1,
                content: "fn main() {".into(),
            },
            IndexedLine {
                file_path: "src/main.rs".into(),
                file_name: "main.rs".into(),
                line_number: 2,
                content: "    println!(\"hello\");".into(),
            },
        ];
        replace_source_lines(&database, "src-1", "/proj", &lines)
            .await
            .unwrap();
        set_source_status(&database, "src-1", "indexed", Some(20), 1, None)
            .await
            .unwrap();
        let stored = read_source(&database, "src-1").await.unwrap().unwrap();
        assert_eq!(stored.status, "indexed");
        assert_eq!(stored.doc_count, 1);

        let hits = search_index_lines(&database, "/proj", "hello", 10, 0)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].line_number, 2);
        // A wildcard in the query is matched literally, not as a wildcard.
        assert!(search_index_lines(&database, "/proj", "%", 10, 0)
            .await
            .unwrap()
            .is_empty());

        // Removing the source drops its indexed lines too.
        delete_project_source(&database, "src-1").await.unwrap();
        assert!(read_project_sources(&database, "/proj")
            .await
            .unwrap()
            .is_empty());
        assert!(search_index_lines(&database, "/proj", "hello", 10, 0)
            .await
            .unwrap()
            .is_empty());
    }
}
