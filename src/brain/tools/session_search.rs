//! Session Search Tool
//!
//! Indexes session message history into the qmd "sessions" collection and
//! searches it using hybrid FTS5 + vector search (same engine as memory_search).
//! Sessions are indexed on-demand with hash-based deduplication — unchanged
//! sessions are skipped instantly.

use super::error::Result;
use super::r#trait::{Tool, ToolCapability, ToolExecutionContext, ToolResult};
use async_trait::async_trait;
use qmd::{Store, hybrid_search_rrf};
use serde_json::Value;
use sqlx::SqlitePool;

const COLLECTION: &str = "sessions";

/// Tool for listing and searching session message history via QMD hybrid search.
pub struct SessionSearchTool {
    pool: SqlitePool,
}

impl SessionSearchTool {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl Tool for SessionSearchTool {
    fn name(&self) -> &str {
        "session_search"
    }

    fn description(&self) -> &str {
        "Search or list chat session history using hybrid FTS5 + vector semantic search. \
         Use 'list' to show all sessions with titles, dates, and message counts. \
         Use 'search' to find messages across sessions by natural-language query. \
         'session' can be a number (1 = most recent), a title keyword, or 'all' (default)."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "operation": {
                    "type": "string",
                    "enum": ["list", "search"],
                    "description": "'list' to show sessions, 'search' to find messages"
                },
                "query": {
                    "type": "string",
                    "description": "Natural-language query (required for 'search')"
                },
                "session": {
                    "type": "string",
                    "description": "Session to search: number (1=most recent), title keyword, or 'all' (default)"
                },
                "n": {
                    "type": "integer",
                    "description": "Max results to return (default: 10)",
                    "default": 10
                }
            },
            "required": ["operation"]
        })
    }

    fn capabilities(&self) -> Vec<ToolCapability> {
        vec![ToolCapability::ReadFiles]
    }

    fn requires_approval(&self) -> bool {
        false
    }

    async fn execute(&self, input: Value, _context: &ToolExecutionContext) -> Result<ToolResult> {
        let operation = input
            .get("operation")
            .and_then(|v| v.as_str())
            .unwrap_or("list");

        match operation {
            "list" => self.list_sessions().await,
            "search" => {
                let query = match input.get("query").and_then(|v| v.as_str()) {
                    Some(q) if !q.is_empty() => q.to_string(),
                    _ => {
                        return Ok(ToolResult::error(
                            "'query' is required for search".to_string(),
                        ));
                    }
                };
                let session_filter = input
                    .get("session")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let n = input
                    .get("n")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as usize;
                self.search_sessions(&query, session_filter.as_deref(), n)
                    .await
            }
            _ => Ok(ToolResult::error(format!(
                "Unknown operation '{}'. Use 'list' or 'search'.",
                operation
            ))),
        }
    }
}

impl SessionSearchTool {
    async fn list_sessions(&self) -> Result<ToolResult> {
        use crate::db::repository::{MessageRepository, SessionListOptions, SessionRepository};

        let session_repo = SessionRepository::new(self.pool.clone());
        let message_repo = MessageRepository::new(self.pool.clone());

        let sessions = session_repo
            .list(SessionListOptions {
                include_archived: false,
                limit: None,
                offset: 0,
            })
            .await
            .map_err(|e| super::error::ToolError::Execution(e.to_string()))?;

        if sessions.is_empty() {
            return Ok(ToolResult::success("No sessions found.".to_string()));
        }

        let mut output = String::new();
        for (i, session) in sessions.iter().enumerate() {
            let count = message_repo
                .count_by_session(session.id)
                .await
                .unwrap_or(0);
            let title = session.title.as_deref().unwrap_or("Untitled");
            let date = session.updated_at.format("%Y-%m-%d").to_string();
            output.push_str(&format!(
                "{}. \"{}\" — {}, {} messages\n",
                i + 1,
                title,
                date,
                count
            ));
        }

        Ok(ToolResult::success(output))
    }

    async fn search_sessions(
        &self,
        query: &str,
        session_filter: Option<&str>,
        n: usize,
    ) -> Result<ToolResult> {
        use crate::db::repository::{MessageRepository, SessionListOptions, SessionRepository};

        let session_repo = SessionRepository::new(self.pool.clone());
        let message_repo = MessageRepository::new(self.pool.clone());

        // Load all sessions (most-recent-first) to resolve filter
        let all_sessions = session_repo
            .list(SessionListOptions {
                include_archived: true,
                limit: None,
                offset: 0,
            })
            .await
            .map_err(|e| super::error::ToolError::Execution(e.to_string()))?;

        let target_sessions: Vec<_> = match session_filter {
            None | Some("all") => all_sessions,
            Some(filter) => {
                if let Ok(idx) = filter.parse::<usize>() {
                    // 1-based index into most-recent-first list
                    all_sessions
                        .into_iter()
                        .nth(idx.saturating_sub(1))
                        .into_iter()
                        .collect()
                } else {
                    // Case-insensitive title substring match
                    let lower = filter.to_lowercase();
                    all_sessions
                        .into_iter()
                        .filter(|s| {
                            s.title
                                .as_deref()
                                .unwrap_or("")
                                .to_lowercase()
                                .contains(&lower)
                        })
                        .collect()
                }
            }
        };

        if target_sessions.is_empty() {
            return Ok(ToolResult::success(
                "No matching sessions found.".to_string(),
            ));
        }

        let store = match crate::memory::get_store() {
            Ok(s) => s,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Session search unavailable: {e}"
                )));
            }
        };

        // Index target sessions into QMD — hash-skipped if content unchanged
        for session in &target_sessions {
            let messages = message_repo
                .find_by_session(session.id)
                .await
                .unwrap_or_default();

            if messages.is_empty() {
                continue;
            }

            let title = session
                .title
                .clone()
                .unwrap_or_else(|| "Untitled".to_string());
            let date = session.updated_at.format("%Y-%m-%d").to_string();
            let mut body =
                format!("# {}\nDate: {}\nSession: {}\n\n", title, date, session.id);

            for msg in &messages {
                let role = if msg.role == "user" {
                    "[user]"
                } else {
                    "[assistant]"
                };
                // Cap individual messages to avoid huge documents
                let content = if msg.content.len() > 2000 {
                    format!("{}...", &msg.content[..2000])
                } else {
                    msg.content.clone()
                };
                body.push_str(&format!("{} {}\n\n", role, content));
            }

            let doc_path = format!("{}.md", session.id);
            let title_owned = title.clone();
            let body_owned = body;

            if let Err(e) = tokio::task::spawn_blocking(move || {
                index_session_body(store, &doc_path, &title_owned, body_owned)
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r)
            {
                tracing::warn!("Failed to index session {}: {}", session.id, e);
            }
        }

        // Session doc paths for post-filter
        let target_paths: Vec<String> = target_sessions
            .iter()
            .map(|s| format!("{}.md", s.id))
            .collect();

        // Title map for output formatting
        let title_map: std::collections::HashMap<String, String> = target_sessions
            .iter()
            .map(|s| {
                (
                    format!("{}.md", s.id),
                    s.title.clone().unwrap_or_else(|| "Untitled".to_string()),
                )
            })
            .collect();

        let fts_query = sanitize_fts_query(query);
        if fts_query.is_empty() {
            return Ok(ToolResult::error("Query cannot be empty.".to_string()));
        }

        let query_owned = query.to_string();
        let results = tokio::task::spawn_blocking(move || {
            search_in_sessions(store, &fts_query, &query_owned, n, &target_paths)
        })
        .await
        .map_err(|e| super::error::ToolError::Execution(e.to_string()))?
        .map_err(super::error::ToolError::Execution)?;

        if results.is_empty() {
            return Ok(ToolResult::success(format!(
                "No messages found matching '{}' in the selected session(s).",
                query
            )));
        }

        let mut output = String::new();
        for (doc_path, snippet) in &results {
            let title = title_map
                .get(doc_path)
                .map(String::as_str)
                .unwrap_or("Untitled");
            output.push_str(&format!("**{}**\n   {}\n\n", title, snippet));
        }

        Ok(ToolResult::success(output))
    }
}

/// Insert/update a session document in the QMD store. Skips if content unchanged.
/// Triggers embedding if the engine is already running (non-blocking, FTS-only fallback).
fn index_session_body(
    store: &'static std::sync::Mutex<Store>,
    doc_path: &str,
    title: &str,
    body: String,
) -> std::result::Result<(), String> {
    let hash = Store::hash_content(&body);
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S")
        .to_string();

    {
        let s = store
            .lock()
            .map_err(|e| format!("Store lock poisoned: {e}"))?;

        if matches!(s.find_active_document(COLLECTION, doc_path), Ok(Some((_, ref h, _))) if h == &hash) {
            return Ok(());
        }

        s.insert_content(&hash, &body, &now)
            .map_err(|e| format!("insert_content failed: {e}"))?;
        s.insert_document(COLLECTION, doc_path, title, &hash, &now, &now)
            .map_err(|e| format!("insert_document failed: {e}"))?;
    }

    // Embed after releasing store lock — engine lock acquired inside embed_content
    crate::memory::embed_content(store, &body);

    Ok(())
}

/// Hybrid FTS5 + vector search in the sessions collection, post-filtered to target paths.
fn search_in_sessions(
    store: &'static std::sync::Mutex<Store>,
    fts_query: &str,
    raw_query: &str,
    n: usize,
    target_paths: &[String],
) -> std::result::Result<Vec<(String, String)>, String> {
    // Non-blocking engine check — if not ready, fall back to FTS-only
    let query_embedding = crate::memory::engine_if_ready().and_then(|em| {
        em.lock()
            .ok()
            .and_then(|mut e| e.embed_query(raw_query).ok().map(|r| r.embedding))
    });

    let s = store
        .lock()
        .map_err(|e| format!("Store lock poisoned: {e}"))?;

    let fts_results = s
        .search_fts(fts_query, n * 3, Some(COLLECTION))
        .map_err(|e| format!("FTS search failed: {e}"))?;

    // Build ranked list via hybrid RRF or FTS-only
    let ranked: Vec<(String, f64, String)> = if let Some(ref emb) = query_embedding {
        let vec_results = s
            .search_vec(emb, n * 3, Some(COLLECTION))
            .unwrap_or_default();

        if !vec_results.is_empty() {
            let fts_tuples: Vec<_> = fts_results
                .iter()
                .map(|r| {
                    let body = s
                        .get_document(&r.doc.collection_name, &r.doc.path)
                        .ok()
                        .flatten()
                        .and_then(|d| d.body)
                        .unwrap_or_default();
                    (
                        r.doc.path.clone(),
                        r.doc.path.clone(),
                        r.doc.title.clone(),
                        body,
                    )
                })
                .collect();

            let vec_tuples: Vec<_> = vec_results
                .iter()
                .map(|r| {
                    let body = s
                        .get_document(&r.doc.collection_name, &r.doc.path)
                        .ok()
                        .flatten()
                        .and_then(|d| d.body)
                        .unwrap_or_default();
                    (
                        r.doc.path.clone(),
                        r.doc.path.clone(),
                        r.doc.title.clone(),
                        body,
                    )
                })
                .collect();

            hybrid_search_rrf(fts_tuples, vec_tuples, 60)
                .into_iter()
                .map(|r| (r.file, r.score, r.body))
                .collect()
        } else {
            fts_results
                .iter()
                .map(|r| {
                    let body = s
                        .get_document(&r.doc.collection_name, &r.doc.path)
                        .ok()
                        .flatten()
                        .and_then(|d| d.body)
                        .unwrap_or_default();
                    (r.doc.path.clone(), r.score, body)
                })
                .collect()
        }
    } else {
        fts_results
            .iter()
            .map(|r| {
                let body = s
                    .get_document(&r.doc.collection_name, &r.doc.path)
                    .ok()
                    .flatten()
                    .and_then(|d| d.body)
                    .unwrap_or_default();
                (r.doc.path.clone(), r.score, body)
            })
            .collect()
    };

    let results = ranked
        .into_iter()
        .filter(|(path, _, _)| target_paths.contains(path))
        .take(n)
        .map(|(path, _, body)| {
            let snippet = extract_snippet(&body, fts_query, 250);
            (path, snippet)
        })
        .collect();

    Ok(results)
}

fn sanitize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|w| {
            let clean: String = w.chars().filter(|c| *c != '"').collect();
            format!("\"{clean}\"")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn extract_snippet(body: &str, query: &str, max_len: usize) -> String {
    let query_lower = query.to_lowercase();
    let body_lower = body.to_lowercase();

    let mut best_pos = 0;
    for word in query_lower.split_whitespace() {
        let clean: String = word.chars().filter(|c| *c != '"').collect();
        if !clean.is_empty()
            && let Some(pos) = body_lower.find(&clean)
        {
            best_pos = pos;
            break;
        }
    }

    let start = best_pos.saturating_sub(50);
    let end = (start + max_len).min(body.len());
    let start = body.floor_char_boundary(start);
    let end = body.ceil_char_boundary(end);

    let mut snippet = String::new();
    if start > 0 {
        snippet.push_str("...");
    }
    snippet.push_str(body[start..end].trim());
    if end < body.len() {
        snippet.push_str("...");
    }

    snippet
}
