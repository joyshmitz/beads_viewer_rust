use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::{BvrError, Result};

pub const SQLITE_EXPORT_FILENAME: &str = "beads.sqlite3";
pub const SQLITE_EXPORT_SCHEMA_VERSION: i32 = 1;
pub const SQLITE_WRITER_CONTRACT_VERSION: &str = "1";
pub const DEFAULT_SQLITE_PAGE_SIZE: u32 = 1_024;

const EXPORT_META_KEYS: &[(&str, &str)] = &[
    ("schema_version", "1"),
    ("writer_contract_version", SQLITE_WRITER_CONTRACT_VERSION),
    ("layout", "single-file"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteBootstrapOptions {
    pub page_size: u32,
}

impl Default for SqliteBootstrapOptions {
    fn default() -> Self {
        Self {
            page_size: DEFAULT_SQLITE_PAGE_SIZE,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqliteBootstrapSummary {
    pub database_path: PathBuf,
    pub schema_version: i32,
    pub writer_contract_version: &'static str,
    pub page_size: u32,
    pub journal_mode: String,
}

#[must_use]
pub fn export_database_path(output_dir: &Path) -> PathBuf {
    output_dir.join(SQLITE_EXPORT_FILENAME)
}

pub fn bootstrap_export_database(
    output_dir: &Path,
    options: &SqliteBootstrapOptions,
) -> Result<SqliteBootstrapSummary> {
    fs::create_dir_all(output_dir)?;

    let database_path = export_database_path(output_dir);
    let connection = Connection::open(&database_path)?;

    configure_connection(&connection, options.page_size)?;
    verify_existing_contract(&connection)?;
    create_schema(&connection)?;
    rebuild_issue_search_index(&connection)?;
    rebuild_issue_overview_mv(&connection)?;
    upsert_contract_meta(&connection, options.page_size)?;

    let actual_page_size = query_pragma_i64(&connection, "page_size")?;
    let journal_mode = query_pragma_string(&connection, "journal_mode")?;

    Ok(SqliteBootstrapSummary {
        database_path,
        schema_version: SQLITE_EXPORT_SCHEMA_VERSION,
        writer_contract_version: SQLITE_WRITER_CONTRACT_VERSION,
        page_size: u32::try_from(actual_page_size).map_err(|_| {
            BvrError::InvalidArgument(format!(
                "sqlite returned an invalid page_size value: {actual_page_size}"
            ))
        })?,
        journal_mode,
    })
}

pub fn rebuild_issue_search_index(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS issues_fts USING fts5(
            id UNINDEXED,
            title,
            description,
            design,
            acceptance_criteria,
            notes,
            labels,
            assignee,
            source_repo,
            content='issues',
            content_rowid='rowid',
            tokenize='porter unicode61'
        );

        INSERT INTO issues_fts(issues_fts) VALUES('rebuild');
        ",
    )?;

    Ok(())
}

pub fn rebuild_issue_overview_mv(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        DELETE FROM issue_overview_mv;

        INSERT INTO issue_overview_mv (
            id,
            title,
            description,
            design,
            acceptance_criteria,
            notes,
            status,
            priority,
            issue_type,
            assignee,
            estimated_minutes,
            labels,
            created_at,
            updated_at,
            due_date,
            closed_at,
            source_repo,
            pagerank,
            betweenness,
            critical_path_depth,
            triage_score,
            blocks_count,
            blocked_by_count,
            blocker_count,
            dependent_count,
            critical_depth,
            in_cycle,
            comment_count,
            blocks_ids,
            blocked_by_ids
        )
        SELECT
            i.id,
            i.title,
            i.description,
            i.design,
            i.acceptance_criteria,
            i.notes,
            i.status,
            i.priority,
            i.issue_type,
            i.assignee,
            i.estimated_minutes,
            i.labels,
            i.created_at,
            i.updated_at,
            i.due_date,
            i.closed_at,
            i.source_repo,
            COALESCE(m.pagerank, 0),
            COALESCE(m.betweenness, 0),
            COALESCE(m.critical_path_depth, 0),
            COALESCE(m.triage_score, 0),
            COALESCE(m.blocks_count, 0),
            COALESCE(m.blocked_by_count, 0),
            COALESCE(m.blocked_by_count, 0),
            COALESCE(m.blocks_count, 0),
            COALESCE(m.critical_path_depth, 0),
            0,
            (
                SELECT COUNT(*)
                FROM comments c
                WHERE c.issue_id = i.id
            ),
            (
                SELECT GROUP_CONCAT(issue_id)
                FROM (
                    SELECT issue_id
                    FROM dependencies
                    WHERE depends_on_id = i.id
                        AND (type = 'blocks' OR type = '')
                    ORDER BY issue_id
                )
            ),
            (
                SELECT GROUP_CONCAT(depends_on_id)
                FROM (
                    SELECT depends_on_id
                    FROM dependencies
                    WHERE issue_id = i.id
                        AND (type = 'blocks' OR type = '')
                    ORDER BY depends_on_id
                )
            )
        FROM issues i
        LEFT JOIN issue_metrics m
            ON i.id = m.issue_id
        ORDER BY i.id;
        ",
    )?;
    Ok(())
}

fn configure_connection(connection: &Connection, page_size: u32) -> Result<()> {
    connection.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA journal_mode = DELETE;
        PRAGMA synchronous = NORMAL;
        ",
    )?;
    connection.pragma_update(None, "page_size", page_size)?;
    Ok(())
}

fn verify_existing_contract(connection: &Connection) -> Result<()> {
    let existing_schema_version = query_pragma_i64(connection, "user_version")?;
    if existing_schema_version != 0
        && existing_schema_version != i64::from(SQLITE_EXPORT_SCHEMA_VERSION)
    {
        return Err(BvrError::InvalidArgument(format!(
            "existing export database uses schema version {existing_schema_version}, expected {SQLITE_EXPORT_SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

fn create_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS issues (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            design TEXT NOT NULL DEFAULT '',
            acceptance_criteria TEXT NOT NULL DEFAULT '',
            notes TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL,
            priority INTEGER NOT NULL,
            issue_type TEXT NOT NULL,
            assignee TEXT NOT NULL DEFAULT '',
            estimated_minutes INTEGER,
            labels TEXT NOT NULL DEFAULT '[]',
            created_at TEXT,
            updated_at TEXT,
            due_date TEXT,
            closed_at TEXT,
            source_repo TEXT NOT NULL DEFAULT '.'
        );

        CREATE TABLE IF NOT EXISTS dependencies (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            issue_id TEXT NOT NULL,
            depends_on_id TEXT NOT NULL,
            type TEXT NOT NULL DEFAULT 'blocks',
            created_by TEXT NOT NULL DEFAULT '',
            created_at TEXT,
            FOREIGN KEY (issue_id) REFERENCES issues(id),
            FOREIGN KEY (depends_on_id) REFERENCES issues(id)
        );

        CREATE TABLE IF NOT EXISTS comments (
            id TEXT PRIMARY KEY,
            issue_id TEXT NOT NULL,
            author TEXT NOT NULL DEFAULT '',
            text TEXT NOT NULL,
            created_at TEXT,
            FOREIGN KEY (issue_id) REFERENCES issues(id)
        );

        CREATE TABLE IF NOT EXISTS issue_metrics (
            issue_id TEXT PRIMARY KEY,
            pagerank REAL NOT NULL DEFAULT 0,
            betweenness REAL NOT NULL DEFAULT 0,
            critical_path_depth INTEGER NOT NULL DEFAULT 0,
            triage_score REAL NOT NULL DEFAULT 0,
            blocks_count INTEGER NOT NULL DEFAULT 0,
            blocked_by_count INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (issue_id) REFERENCES issues(id)
        );

        CREATE TABLE IF NOT EXISTS triage_recommendations (
            issue_id TEXT PRIMARY KEY,
            score REAL NOT NULL,
            action TEXT NOT NULL,
            reasons TEXT NOT NULL DEFAULT '[]',
            unblocks_ids TEXT NOT NULL DEFAULT '[]',
            blocked_by_ids TEXT NOT NULL DEFAULT '[]',
            FOREIGN KEY (issue_id) REFERENCES issues(id)
        );

        CREATE TABLE IF NOT EXISTS export_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS issue_overview_mv (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            design TEXT NOT NULL DEFAULT '',
            acceptance_criteria TEXT NOT NULL DEFAULT '',
            notes TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL,
            priority INTEGER NOT NULL,
            issue_type TEXT NOT NULL,
            assignee TEXT NOT NULL DEFAULT '',
            estimated_minutes INTEGER,
            labels TEXT NOT NULL DEFAULT '[]',
            created_at TEXT,
            updated_at TEXT,
            due_date TEXT,
            closed_at TEXT,
            source_repo TEXT NOT NULL DEFAULT '.',
            pagerank REAL NOT NULL DEFAULT 0,
            betweenness REAL NOT NULL DEFAULT 0,
            critical_path_depth INTEGER NOT NULL DEFAULT 0,
            triage_score REAL NOT NULL DEFAULT 0,
            blocks_count INTEGER NOT NULL DEFAULT 0,
            blocked_by_count INTEGER NOT NULL DEFAULT 0,
            blocker_count INTEGER NOT NULL DEFAULT 0,
            dependent_count INTEGER NOT NULL DEFAULT 0,
            critical_depth INTEGER NOT NULL DEFAULT 0,
            in_cycle INTEGER NOT NULL DEFAULT 0,
            comment_count INTEGER NOT NULL DEFAULT 0,
            blocks_ids TEXT,
            blocked_by_ids TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_issues_status
            ON issues(status);
        CREATE INDEX IF NOT EXISTS idx_issues_priority
            ON issues(priority, status);
        CREATE INDEX IF NOT EXISTS idx_issues_updated
            ON issues(updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_issues_type_status
            ON issues(issue_type, status);
        CREATE INDEX IF NOT EXISTS idx_issues_source_repo
            ON issues(source_repo, status);

        CREATE INDEX IF NOT EXISTS idx_deps_issue
            ON dependencies(issue_id);
        CREATE INDEX IF NOT EXISTS idx_deps_depends
            ON dependencies(depends_on_id);
        CREATE INDEX IF NOT EXISTS idx_deps_type
            ON dependencies(type);

        CREATE INDEX IF NOT EXISTS idx_comments_issue
            ON comments(issue_id);
        CREATE INDEX IF NOT EXISTS idx_comments_created
            ON comments(created_at DESC);

        CREATE INDEX IF NOT EXISTS idx_metrics_score
            ON issue_metrics(triage_score DESC);
        CREATE INDEX IF NOT EXISTS idx_metrics_pagerank
            ON issue_metrics(pagerank DESC);

        CREATE INDEX IF NOT EXISTS idx_mv_status
            ON issue_overview_mv(status);
        CREATE INDEX IF NOT EXISTS idx_mv_priority
            ON issue_overview_mv(priority);
        CREATE INDEX IF NOT EXISTS idx_mv_score
            ON issue_overview_mv(triage_score DESC);
        ",
    )?;
    connection.pragma_update(None, "user_version", SQLITE_EXPORT_SCHEMA_VERSION)?;
    Ok(())
}

fn upsert_contract_meta(connection: &Connection, page_size: u32) -> Result<()> {
    for (key, value) in EXPORT_META_KEYS {
        connection.execute(
            "INSERT INTO export_meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [*key, *value],
        )?;
    }

    let actual_page_size = query_pragma_i64(connection, "page_size")?;
    let page_size_value = if actual_page_size > 0 {
        actual_page_size.to_string()
    } else {
        page_size.to_string()
    };
    connection.execute(
        "INSERT INTO export_meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["page_size", page_size_value.as_str()],
    )?;

    let journal_mode = query_pragma_string(connection, "journal_mode")?;
    connection.execute(
        "INSERT INTO export_meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        ["journal_mode", journal_mode.as_str()],
    )?;
    Ok(())
}

fn query_pragma_i64(connection: &Connection, pragma_name: &str) -> Result<i64> {
    let sql = format!("PRAGMA {pragma_name}");
    Ok(connection.query_row(&sql, [], |row| row.get(0))?)
}

fn query_pragma_string(connection: &Connection, pragma_name: &str) -> Result<String> {
    let sql = format!("PRAGMA {pragma_name}");
    Ok(connection.query_row(&sql, [], |row| row.get(0))?)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use rusqlite::params;
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn bootstrap_export_database_creates_expected_schema_inventory() {
        let temp = tempdir().expect("tempdir");
        let summary = bootstrap_export_database(temp.path(), &SqliteBootstrapOptions::default())
            .expect("bootstrap sqlite export database");

        assert_eq!(
            summary.database_path,
            temp.path().join(SQLITE_EXPORT_FILENAME)
        );
        assert_eq!(summary.schema_version, SQLITE_EXPORT_SCHEMA_VERSION);
        assert_eq!(
            summary.writer_contract_version,
            SQLITE_WRITER_CONTRACT_VERSION
        );
        assert_eq!(summary.page_size, DEFAULT_SQLITE_PAGE_SIZE);
        assert_eq!(summary.journal_mode, "delete");

        let connection =
            Connection::open(&summary.database_path).expect("open bootstrapped sqlite database");

        assert_eq!(
            query_pragma_i64(&connection, "user_version").expect("pragma user_version"),
            i64::from(SQLITE_EXPORT_SCHEMA_VERSION)
        );
        assert_eq!(
            query_pragma_i64(&connection, "page_size").expect("pragma page_size"),
            i64::from(DEFAULT_SQLITE_PAGE_SIZE)
        );

        let object_names = sqlite_object_inventory(&connection);

        for table in [
            "issues",
            "dependencies",
            "comments",
            "issue_metrics",
            "triage_recommendations",
            "export_meta",
            "issue_overview_mv",
            "issues_fts",
        ] {
            assert!(
                object_names.contains(table),
                "missing sqlite object {table}; inventory: {object_names:?}"
            );
        }

        for index in [
            "idx_issues_status",
            "idx_issues_priority",
            "idx_issues_updated",
            "idx_issues_type_status",
            "idx_issues_source_repo",
            "idx_deps_issue",
            "idx_deps_depends",
            "idx_deps_type",
            "idx_comments_issue",
            "idx_comments_created",
            "idx_metrics_score",
            "idx_metrics_pagerank",
            "idx_mv_status",
            "idx_mv_priority",
            "idx_mv_score",
        ] {
            assert!(
                object_names.contains(index),
                "missing sqlite object {index}; inventory: {object_names:?}"
            );
        }

        let meta = export_meta(&connection);
        assert_eq!(meta.get("schema_version"), Some(&"1".to_string()));
        assert_eq!(
            meta.get("writer_contract_version"),
            Some(&SQLITE_WRITER_CONTRACT_VERSION.to_string())
        );
        assert_eq!(
            meta.get("page_size"),
            Some(&DEFAULT_SQLITE_PAGE_SIZE.to_string())
        );
        assert_eq!(meta.get("journal_mode"), Some(&"delete".to_string()));
        assert_eq!(meta.get("layout"), Some(&"single-file".to_string()));
    }

    #[test]
    fn bootstrap_export_database_is_idempotent() {
        let temp = tempdir().expect("tempdir");
        bootstrap_export_database(temp.path(), &SqliteBootstrapOptions::default())
            .expect("first bootstrap");
        let second = bootstrap_export_database(temp.path(), &SqliteBootstrapOptions::default())
            .expect("second bootstrap");

        let connection =
            Connection::open(&second.database_path).expect("open bootstrapped sqlite database");

        let issue_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM issue_overview_mv", [], |row| {
                row.get(0)
            })
            .expect("issue_overview_mv count");
        let fts_count: i64 = connection
            .query_row("SELECT COUNT(*) FROM issues_fts", [], |row| row.get(0))
            .expect("issues_fts count");

        assert_eq!(issue_count, 0);
        assert_eq!(fts_count, 0);
        assert_eq!(export_meta(&connection).len(), 5);
    }

    #[test]
    fn bootstrap_export_database_rejects_conflicting_schema_version() {
        let temp = tempdir().expect("tempdir");
        let db_path = export_database_path(temp.path());
        let connection = Connection::open(&db_path).expect("open sqlite database");
        connection
            .pragma_update(None, "user_version", 99_i32)
            .expect("set conflicting schema version");
        drop(connection);

        let err = bootstrap_export_database(temp.path(), &SqliteBootstrapOptions::default())
            .expect_err("conflicting schema version should fail");
        assert!(
            err.to_string().contains("schema version 99"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rebuild_issue_overview_mv_derives_counts_and_aliases() {
        let temp = tempdir().expect("tempdir");
        let summary = bootstrap_export_database(temp.path(), &SqliteBootstrapOptions::default())
            .expect("bootstrap sqlite export database");
        let connection =
            Connection::open(&summary.database_path).expect("open bootstrapped sqlite database");

        connection
            .execute(
                "
                INSERT INTO issues (
                    id, title, description, design, acceptance_criteria, notes, status,
                    priority, issue_type, assignee, estimated_minutes, labels, created_at,
                    updated_at, due_date, closed_at, source_repo
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    "ISSUE-1",
                    "Export SQLite",
                    "Ship the export schema",
                    "Use a dedicated module",
                    "Schema is explicit",
                    "Keep bootstrap deterministic",
                    "open",
                    1_i32,
                    "task",
                    "alex",
                    45_i32,
                    "[\"export\",\"sqlite\"]",
                    "2026-03-08T18:00:00Z",
                    "2026-03-08T18:30:00Z",
                    "2026-03-10T00:00:00Z",
                    Option::<String>::None,
                    "core"
                ],
            )
            .expect("insert issue");
        connection
            .execute(
                "
                INSERT INTO issues (
                    id, title, description, design, acceptance_criteria, notes, status,
                    priority, issue_type, assignee, estimated_minutes, labels, created_at,
                    updated_at, due_date, closed_at, source_repo
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    "ISSUE-2",
                    "Downstream consumer",
                    "",
                    "",
                    "",
                    "",
                    "blocked",
                    2_i32,
                    "task",
                    "",
                    Option::<i32>::None,
                    "[]",
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    "core"
                ],
            )
            .expect("insert second issue");
        connection
            .execute(
                "
                INSERT INTO dependencies (issue_id, depends_on_id, type, created_by, created_at)
                VALUES (?, ?, ?, ?, ?)
                ",
                params![
                    "ISSUE-2",
                    "ISSUE-1",
                    "blocks",
                    "tester",
                    "2026-03-08T18:31:00Z"
                ],
            )
            .expect("insert dependency");
        connection
            .execute(
                "
                INSERT INTO comments (id, issue_id, author, text, created_at)
                VALUES (?, ?, ?, ?, ?)
                ",
                params![
                    "ISSUE-1:1",
                    "ISSUE-1",
                    "alex",
                    "Need the schema locked down before population.",
                    "2026-03-08T18:40:00Z"
                ],
            )
            .expect("insert comment");
        connection
            .execute(
                "
                INSERT INTO issue_metrics (
                    issue_id, pagerank, betweenness, critical_path_depth,
                    triage_score, blocks_count, blocked_by_count
                )
                VALUES (?, ?, ?, ?, ?, ?, ?)
                ",
                params!["ISSUE-1", 0.25_f64, 0.1_f64, 4_i32, 0.8_f64, 1_i32, 0_i32],
            )
            .expect("insert metrics");

        rebuild_issue_overview_mv(&connection).expect("rebuild issue_overview_mv");

        let derived = connection
            .query_row(
                "
                SELECT
                    blocker_count,
                    dependent_count,
                    critical_depth,
                    comment_count,
                    blocks_ids,
                    blocked_by_ids,
                    source_repo,
                    design,
                    acceptance_criteria,
                    notes
                FROM issue_overview_mv
                WHERE id = ?
                ",
                ["ISSUE-1"],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, String>(9)?,
                    ))
                },
            )
            .expect("query issue_overview_mv");

        assert_eq!(derived.0, 0);
        assert_eq!(derived.1, 1);
        assert_eq!(derived.2, 4);
        assert_eq!(derived.3, 1);
        assert_eq!(derived.4.as_deref(), Some("ISSUE-2"));
        assert_eq!(derived.5, None);
        assert_eq!(derived.6, "core");
        assert_eq!(derived.7, "Use a dedicated module");
        assert_eq!(derived.8, "Schema is explicit");
        assert_eq!(derived.9, "Keep bootstrap deterministic");
    }

    #[test]
    fn rebuild_issue_search_index_supports_prefix_queries() {
        let temp = tempdir().expect("tempdir");
        let summary = bootstrap_export_database(temp.path(), &SqliteBootstrapOptions::default())
            .expect("bootstrap sqlite export database");
        let connection =
            Connection::open(&summary.database_path).expect("open bootstrapped sqlite database");

        connection
            .execute(
                "
                INSERT INTO issues (
                    id, title, description, design, acceptance_criteria, notes, status,
                    priority, issue_type, assignee, estimated_minutes, labels, created_at,
                    updated_at, due_date, closed_at, source_repo
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                ",
                params![
                    "SEARCH-1",
                    "Authentication hardening",
                    "Tighten export viewer auth checks",
                    "Keep search parity with legacy",
                    "",
                    "",
                    "open",
                    2_i32,
                    "feature",
                    "sam",
                    Option::<i32>::None,
                    "[\"auth\",\"export\"]",
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    Option::<String>::None,
                    "security"
                ],
            )
            .expect("insert searchable issue");

        rebuild_issue_search_index(&connection).expect("rebuild issues_fts");

        let exact_matches: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM issues_fts WHERE issues_fts MATCH 'authentication'",
                [],
                |row| row.get(0),
            )
            .expect("exact fts query");
        let prefix_matches: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM issues_fts WHERE issues_fts MATCH 'hard*'",
                [],
                |row| row.get(0),
            )
            .expect("prefix fts query");

        assert_eq!(exact_matches, 1);
        assert_eq!(prefix_matches, 1);
    }

    fn sqlite_object_inventory(connection: &Connection) -> BTreeSet<String> {
        let mut stmt = connection
            .prepare(
                "
                SELECT name
                FROM sqlite_master
                WHERE type IN ('table', 'index')
                    AND name NOT LIKE 'sqlite_%'
                ORDER BY name
                ",
            )
            .expect("prepare sqlite inventory query");
        let names = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query sqlite inventory");
        names
            .collect::<std::result::Result<BTreeSet<_>, _>>()
            .expect("collect sqlite inventory")
    }

    fn export_meta(connection: &Connection) -> BTreeMap<String, String> {
        let mut stmt = connection
            .prepare("SELECT key, value FROM export_meta ORDER BY key")
            .expect("prepare export_meta query");
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .expect("query export_meta");
        rows.collect::<std::result::Result<BTreeMap<_, _>, _>>()
            .expect("collect export_meta")
    }
}
