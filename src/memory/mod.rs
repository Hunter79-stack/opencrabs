//! Memory Module
//!
//! Provides long-term memory search via built-in FTS5 full-text search.
//! Uses the existing `sqlx` SQLite dependency — zero external tools needed.
//! Memory logs (`~/.opencrabs/memory/YYYY-MM-DD.md`) are indexed into an
//! FTS5 virtual table for fast BM25-ranked retrieval.

use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use tokio::sync::OnceCell;

/// A single search result from the FTS5 index.
#[derive(Debug, Clone)]
pub struct MemoryResult {
    pub path: String,
    pub snippet: String,
    pub rank: f64,
}

/// Lazy-initialized singleton pool for the memory database.
static MEMORY_POOL: OnceCell<SqlitePool> = OnceCell::const_new();

/// Get (or create) the shared memory database pool.
///
/// The database lives at `~/.opencrabs/memory/memory.db`.
/// First call initializes the schema (tables + FTS5 virtual table).
pub async fn get_pool() -> Result<&'static SqlitePool, String> {
    MEMORY_POOL
        .get_or_try_init(|| async {
            let db_path = memory_dir().join("memory.db");

            // Ensure directory exists
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create memory dir: {e}"))?;
            }

            let url = format!("sqlite://{}?mode=rwc", db_path.display());

            let pool = SqlitePoolOptions::new()
                .max_connections(2)
                .acquire_timeout(std::time::Duration::from_secs(5))
                .after_connect(|conn, _meta| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA busy_timeout = 3000")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA journal_mode = WAL")
                            .execute(&mut *conn)
                            .await?;
                        Ok(())
                    })
                })
                .connect(&url)
                .await
                .map_err(|e| format!("Failed to connect to memory DB: {e}"))?;

            init_db(&pool).await?;

            tracing::info!("Memory FTS5 database ready at {}", db_path.display());
            Ok(pool)
        })
        .await
}

/// Create the schema: content table + FTS5 virtual table + sync triggers.
async fn init_db(pool: &SqlitePool) -> Result<(), String> {
    // Content table
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS memory_docs (
            id          INTEGER PRIMARY KEY,
            path        TEXT UNIQUE NOT NULL,
            body        TEXT NOT NULL,
            hash        TEXT NOT NULL,
            modified_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create memory_docs: {e}"))?;

    // FTS5 virtual table (external-content backed by memory_docs)
    sqlx::query(
        "CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
            body,
            content=memory_docs,
            content_rowid=id,
            tokenize='porter unicode61'
        )",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create memory_fts: {e}"))?;

    // Triggers to keep FTS in sync with content table
    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS memory_ai AFTER INSERT ON memory_docs BEGIN
            INSERT INTO memory_fts(rowid, body) VALUES (new.id, new.body);
        END",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create insert trigger: {e}"))?;

    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS memory_ad AFTER DELETE ON memory_docs BEGIN
            INSERT INTO memory_fts(memory_fts, rowid, body) VALUES('delete', old.id, old.body);
        END",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create delete trigger: {e}"))?;

    sqlx::query(
        "CREATE TRIGGER IF NOT EXISTS memory_au AFTER UPDATE ON memory_docs BEGIN
            INSERT INTO memory_fts(memory_fts, rowid, body) VALUES('delete', old.id, old.body);
            INSERT INTO memory_fts(rowid, body) VALUES (new.id, new.body);
        END",
    )
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create update trigger: {e}"))?;

    Ok(())
}

/// Full-text search across memory logs using FTS5 BM25 ranking.
///
/// Returns up to `n` results sorted by relevance.
pub async fn search(pool: &SqlitePool, query: &str, n: usize) -> Result<Vec<MemoryResult>, String> {
    // Sanitize the query for FTS5: wrap each word in double quotes to avoid
    // syntax errors from special characters, then join with spaces (implicit AND).
    let fts_query: String = query
        .split_whitespace()
        .map(|w| {
            let clean: String = w.chars().filter(|c| *c != '"').collect();
            format!("\"{}\"", clean)
        })
        .collect::<Vec<_>>()
        .join(" ");

    if fts_query.is_empty() {
        return Ok(vec![]);
    }

    let rows = sqlx::query(
        "SELECT d.path, snippet(memory_fts, 0, '>>>', '<<<', '...', 64) AS snip, rank
         FROM memory_fts f
         JOIN memory_docs d ON d.id = f.rowid
         WHERE memory_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    )
    .bind(&fts_query)
    .bind(n as i64)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("FTS5 search failed: {e}"))?;

    Ok(rows
        .into_iter()
        .map(|r| MemoryResult {
            path: r.get("path"),
            snippet: r.get("snip"),
            rank: r.get("rank"),
        })
        .collect())
}

/// Index a single `.md` file into the FTS5 database.
///
/// Skips re-indexing if the file's SHA-256 hash hasn't changed.
pub async fn index_file(pool: &SqlitePool, path: &Path) -> Result<(), String> {
    let body = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;

    let hash = content_hash(&body);
    let path_str = path.to_string_lossy().to_string();

    // Check if already indexed with same hash
    let existing: Option<String> = sqlx::query_scalar("SELECT hash FROM memory_docs WHERE path = ?1")
        .bind(&path_str)
        .fetch_optional(pool)
        .await
        .map_err(|e| format!("Hash check failed: {e}"))?;

    if existing.as_deref() == Some(&hash) {
        return Ok(()); // unchanged
    }

    let modified = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    sqlx::query(
        "INSERT INTO memory_docs (path, body, hash, modified_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(path) DO UPDATE SET body=?2, hash=?3, modified_at=?4",
    )
    .bind(&path_str)
    .bind(&body)
    .bind(&hash)
    .bind(&modified)
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to index {}: {e}", path.display()))?;

    tracing::debug!("Indexed memory file: {}", path.display());
    Ok(())
}

/// Walk `~/.opencrabs/memory/*.md` and index all files.
///
/// Also prunes entries for files that no longer exist on disk.
/// Returns the number of files indexed.
pub async fn reindex(pool: &SqlitePool) -> Result<usize, String> {
    let dir = memory_dir();
    if !dir.exists() {
        return Ok(0);
    }

    let mut indexed = 0usize;
    let mut on_disk: Vec<String> = Vec::new();

    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Failed to read memory dir: {e}"))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            on_disk.push(path.to_string_lossy().to_string());
            if let Err(e) = index_file(pool, &path).await {
                tracing::warn!("Failed to index {}: {}", path.display(), e);
            } else {
                indexed += 1;
            }
        }
    }

    // Prune deleted files
    let db_paths: Vec<String> =
        sqlx::query_scalar("SELECT path FROM memory_docs")
            .fetch_all(pool)
            .await
            .map_err(|e| format!("Failed to list indexed paths: {e}"))?;

    for db_path in db_paths {
        if !on_disk.contains(&db_path) {
            let _ = sqlx::query("DELETE FROM memory_docs WHERE path = ?1")
                .bind(&db_path)
                .execute(pool)
                .await;
            tracing::debug!("Pruned missing memory file: {}", db_path);
        }
    }

    tracing::info!("Memory reindex complete: {} files", indexed);
    Ok(indexed)
}

/// Fast content hash for change detection (not cryptographic).
fn content_hash(s: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Path to the memory directory: `~/.opencrabs/memory/`
fn memory_dir() -> PathBuf {
    crate::config::opencrabs_home().join("memory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_dir() {
        let dir = memory_dir();
        assert!(dir.to_string_lossy().contains("memory"));
    }

    #[test]
    fn test_content_hash() {
        let hash = content_hash("hello");
        assert_eq!(hash.len(), 16); // u64 → 16 hex chars
        // Deterministic
        assert_eq!(hash, content_hash("hello"));
        // Different input → different hash
        assert_ne!(hash, content_hash("world"));
    }

    #[tokio::test]
    async fn test_init_and_search_empty() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&pool).await.unwrap();

        let results = search(&pool, "nonexistent query", 5).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_index_and_search() {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        init_db(&pool).await.unwrap();

        // Create a temp file
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("2024-01-01.md");
        tokio::fs::write(&file, "# Today\nFixed the authentication bug in login flow")
            .await
            .unwrap();

        // Index it
        index_file(&pool, &file).await.unwrap();

        // Search should find it
        let results = search(&pool, "authentication bug", 5).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].snippet.contains("authentication"));

        // Re-indexing same content should be a no-op (hash match)
        index_file(&pool, &file).await.unwrap();

        // Update content and re-index
        tokio::fs::write(&file, "# Today\nRefactored the database layer")
            .await
            .unwrap();
        index_file(&pool, &file).await.unwrap();

        let results = search(&pool, "database refactor", 5).await.unwrap();
        assert!(!results.is_empty());
    }

}
