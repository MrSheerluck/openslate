use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::db::{self, FromRow, Row};

#[derive(Deserialize)]
pub struct SearchParams {
    q: String,
}

#[derive(Serialize)]
pub struct SearchResult {
    id: String,
    title: String,
    slug: String,
    created_at: String,
    updated_at: String,
    title_highlight: Option<String>,
    content_snippet: Option<String>,
}

impl FromRow for SearchResult {
    fn from_row(row: &Row) -> db::DbResult<Self> {
        Ok(Self {
            id: row.str("id")?,
            title: row.str("title")?,
            slug: row.str("slug")?,
            created_at: row.str("created_at")?,
            updated_at: row.str("updated_at")?,
            title_highlight: row.opt_str("title_highlight")?,
            content_snippet: row.opt_str("content_snippet")?,
        })
    }
}

fn build_fts_query(raw: &str) -> String {
    raw.split_whitespace()
        .map(|word| {
            let word = word.trim_matches('"');
            if word.len() >= 3 {
                format!("{}*", word)
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn search_notes(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let query = params.q.trim();
    if query.is_empty() {
        return Ok(Json(vec![]));
    }

    let fts_query = build_fts_query(query);

    let results = state
        .db
        .row_all::<SearchResult>(
            "SELECT n.id, n.title, n.slug, n.created_at, n.updated_at,
                    highlight(notes_fts, 1, '<mark>', '</mark>') as title_highlight,
                    snippet(notes_fts, 2, '<mark>', '</mark>', '...', 64) as content_snippet
             FROM notes_fts
             JOIN notes n ON n.id = notes_fts.id
             WHERE notes_fts MATCH ?
             ORDER BY rank
             LIMIT 20",
            &[fts_query.into()],
        )
        .await
        .unwrap_or_default();

    Ok(Json(results))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    async fn setup_db() -> Database {
        let db = db::SqlxDatabase::new("sqlite::memory:").await;
        let db = Database::Sqlite(db);

        db.execute(
            "CREATE TABLE notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                slug TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            &[],
        )
        .await
        .unwrap();

        db.execute(
            "CREATE VIRTUAL TABLE notes_fts USING fts5(
                id UNINDEXED,
                title,
                content,
                tokenize='porter unicode61'
            )",
            &[],
        )
        .await
        .unwrap();

        db.execute(
            "CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
                INSERT INTO notes_fts (id, title, content) VALUES (new.id, new.title, new.content);
            END",
            &[],
        )
        .await
        .unwrap();

        db.execute(
            "INSERT INTO notes (id, title, slug, content) VALUES ('1', 'Hello World', 'hello-world', 'This is a test note')",
            &[],
        )
        .await
        .unwrap();

        db
    }

    #[tokio::test]
    async fn test_search_finds_matching() {
        let db = setup_db().await;
        let params = SearchParams { q: "hello".into() };

        // Build a state manually for testing
        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };

        let results = search_notes(State(state), Query(params)).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Hello World");
    }

    #[tokio::test]
    async fn test_search_empty_query() {
        let db = setup_db().await;
        let params = SearchParams { q: "".into() };

        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };
        let results = search_notes(State(state), Query(params)).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_no_match() {
        let db = setup_db().await;
        let params = SearchParams {
            q: "zzznotfound".into(),
        };

        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };
        let results = search_notes(State(state), Query(params)).await.unwrap();
        assert!(results.is_empty());
    }
}
