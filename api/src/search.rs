use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Deserialize)]
pub struct SearchParams {
    q: String,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct SearchResult {
    id: String,
    title: String,
    slug: String,
    created_at: String,
    updated_at: String,
    title_highlight: Option<String>,
    content_snippet: Option<String>,
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
    State(db): State<SqlitePool>,
    Query(params): Query<SearchParams>,
) -> Result<Json<Vec<SearchResult>>, StatusCode> {
    let query = params.q.trim();
    if query.is_empty() {
        return Ok(Json(vec![]));
    }

    let fts_query = build_fts_query(query);

    let results = sqlx::query_as::<_, SearchResult>(
        "SELECT n.id, n.title, n.slug, n.created_at, n.updated_at,
                highlight(notes_fts, 1, '<mark>', '</mark>') as title_highlight,
                snippet(notes_fts, 2, '<mark>', '</mark>', '...', 64) as content_snippet
         FROM notes_fts
         JOIN notes n ON n.id = notes_fts.id
         WHERE notes_fts MATCH ?
         ORDER BY rank
         LIMIT 20",
    )
    .bind(&fts_query)
    .fetch_all(&db)
    .await
    .unwrap_or_default();

    Ok(Json(results))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("failed to create pool");

        sqlx::query(
            "CREATE TABLE notes (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                slug TEXT UNIQUE NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE VIRTUAL TABLE notes_fts USING fts5(
                id UNINDEXED,
                title,
                content,
                tokenize='porter unicode61'
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TRIGGER IF NOT EXISTS notes_ai AFTER INSERT ON notes BEGIN
                INSERT INTO notes_fts (id, title, content) VALUES (new.id, new.title, new.content);
            END",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert a sample note (trigger populates notes_fts)
        sqlx::query(
            "INSERT INTO notes (id, title, slug, content) VALUES ('1', 'Hello World', 'hello-world', 'This is a test note')",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_search_finds_matching() {
        let db = setup_db().await;
        let params = SearchParams { q: "hello".into() };
        let results = search_notes(State(db), Query(params)).await.unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].title, "Hello World");
    }

    #[tokio::test]
    async fn test_search_empty_query() {
        let db = setup_db().await;
        let params = SearchParams { q: "".into() };
        let results = search_notes(State(db), Query(params)).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_no_match() {
        let db = setup_db().await;
        let params = SearchParams {
            q: "zzznotfound".into(),
        };
        let results = search_notes(State(db), Query(params)).await.unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_fts_short_word() {
        assert_eq!(build_fts_query("hi"), "hi");
    }

    #[test]
    fn test_fts_long_word() {
        assert_eq!(build_fts_query("hello"), "hello*");
    }

    #[test]
    fn test_fts_multi_word() {
        assert_eq!(build_fts_query("hello world"), "hello* world*");
    }

    #[test]
    fn test_fts_mixed_lengths() {
        assert_eq!(build_fts_query("ab test"), "ab test*");
    }

    #[test]
    fn test_fts_empty_string() {
        assert_eq!(build_fts_query(""), "");
    }

    #[test]
    fn test_fts_quoted_phrase() {
        assert_eq!(build_fts_query("\"hello world\""), "hello* world*");
    }
}
