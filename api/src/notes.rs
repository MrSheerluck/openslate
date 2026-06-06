use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use uuid::Uuid;

#[derive(Serialize, sqlx::FromRow)]
struct NoteRow {
    id: String,
    title: String,
    slug: String,
    content: String,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
pub struct CreateNote {
    pub title: String,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub struct UpdateNote {
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct LinkInfo {
    pub title: String,
    pub slug: String,
}

#[derive(Serialize)]
pub struct NoteResponse {
    id: String,
    title: String,
    slug: String,
    content: String,
    tags: Vec<String>,
    backlinks: Vec<LinkInfo>,
    created_at: String,
    updated_at: String,
}

#[derive(Serialize)]
pub struct NoteSummary {
    id: String,
    title: String,
    slug: String,
    tags: Vec<String>,
    created_at: String,
    updated_at: String,
}

fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else if c.is_whitespace() || c == '_' {
                '-'
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "untitled".into()
    } else {
        slug
    }
}

async fn ensure_unique_slug(pool: &SqlitePool, slug: &str, exclude_id: Option<&str>) -> String {
    let mut candidate = slug.to_string();
    let mut counter = 1;
    loop {
        let exists = if let Some(id) = exclude_id {
            let count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM notes WHERE slug = ?1 AND id != ?2",
            )
            .bind(&candidate)
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
            count > 0
        } else {
            let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM notes WHERE slug = ?")
                .bind(&candidate)
                .fetch_one(pool)
                .await
                .unwrap_or(0);
            count > 0
        };
        if !exists {
            return candidate;
        }
        candidate = format!("{}-{}", slug, counter);
        counter += 1;
    }
}

async fn get_note_tags(pool: &SqlitePool, note_id: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT t.name FROM tags t
         JOIN note_tags nt ON nt.tag_id = t.id
         WHERE nt.note_id = ? ORDER BY t.name",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

async fn get_backlinks(pool: &SqlitePool, note_id: &str) -> Vec<LinkInfo> {
    sqlx::query_as::<_, LinkInfo>(
        "SELECT n.title, n.slug FROM notes n
         JOIN note_links nl ON nl.source_note_id = n.id
         WHERE nl.target_note_id = ? ORDER BY n.title",
    )
    .bind(note_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

fn parse_wikilinks(content: &str) -> Vec<String> {
    if content.is_empty() {
        return vec![];
    }
    content
        .split("[[")
        .skip(1)
        .filter_map(|s| s.split("]]").next())
        .map(|s| s.trim().to_lowercase().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

async fn update_wikilinks(pool: &SqlitePool, note_id: &str, content: &str) {
    sqlx::query("DELETE FROM note_links WHERE source_note_id = ?")
        .bind(note_id)
        .execute(pool)
        .await
        .ok();

    for slug in &parse_wikilinks(content) {
        let target_id: Option<String> = sqlx::query_scalar("SELECT id FROM notes WHERE slug = ?")
            .bind(slug)
            .fetch_optional(pool)
            .await
            .unwrap_or(None);

        sqlx::query(
            "INSERT OR IGNORE INTO note_links (source_note_id, target_note_id) VALUES (?, ?)",
        )
        .bind(note_id)
        .bind(target_id)
        .execute(pool)
        .await
        .ok();
    }
}

async fn set_note_tags(pool: &SqlitePool, note_id: &str, tags: Option<Vec<String>>) {
    let Some(tags) = tags else { return };

    sqlx::query("DELETE FROM note_tags WHERE note_id = ?")
        .bind(note_id)
        .execute(pool)
        .await
        .ok();

    for name in &tags {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        sqlx::query("INSERT OR IGNORE INTO tags (id, name) VALUES (?, ?)")
            .bind(Uuid::new_v4().to_string())
            .bind(name)
            .execute(pool)
            .await
            .ok();

        if let Some(tag_id) = sqlx::query_scalar::<_, String>("SELECT id FROM tags WHERE name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await
            .unwrap_or(None)
        {
            sqlx::query("INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?, ?)")
                .bind(note_id)
                .bind(tag_id)
                .execute(pool)
                .await
                .ok();
        }
    }
}

pub async fn list_notes(
    State(db): State<SqlitePool>,
) -> Result<Json<Vec<NoteSummary>>, StatusCode> {
    let notes = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, slug, content, created_at, updated_at FROM notes ORDER BY updated_at DESC",
    )
    .fetch_all(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::new();
    for note in notes {
        let tags = get_note_tags(&db, &note.id).await;
        result.push(NoteSummary {
            id: note.id,
            title: note.title,
            slug: note.slug,
            tags,
            created_at: note.created_at,
            updated_at: note.updated_at,
        });
    }
    Ok(Json(result))
}

pub async fn get_note(
    State(db): State<SqlitePool>,
    Path(slug): Path<String>,
) -> Result<Json<NoteResponse>, StatusCode> {
    let note = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, slug, content, created_at, updated_at FROM notes WHERE slug = ?",
    )
    .bind(&slug)
    .fetch_optional(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let tags = get_note_tags(&db, &note.id).await;
    let backlinks = get_backlinks(&db, &note.id).await;

    Ok(Json(NoteResponse {
        id: note.id,
        title: note.title,
        slug: note.slug,
        content: note.content,
        tags,
        backlinks,
        created_at: note.created_at,
        updated_at: note.updated_at,
    }))
}

pub async fn create_note(
    State(db): State<SqlitePool>,
    Json(body): Json<CreateNote>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let id = Uuid::new_v4().to_string();
    let slug = slugify(&body.title);
    let unique_slug = ensure_unique_slug(&db, &slug, None).await;
    let content = body.content.unwrap_or_default();

    sqlx::query("INSERT INTO notes (id, title, slug, content) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(&body.title)
        .bind(&unique_slug)
        .bind(&content)
        .execute(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    set_note_tags(&db, &id, body.tags).await;
    update_wikilinks(&db, &id, &content).await;

    Ok((StatusCode::CREATED, Json(json!({ "slug": unique_slug }))))
}

pub async fn update_note(
    State(db): State<SqlitePool>,
    Path(slug): Path<String>,
    Json(body): Json<UpdateNote>,
) -> Result<Json<Value>, StatusCode> {
    let existing = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, slug, content, created_at, updated_at FROM notes WHERE slug = ?",
    )
    .bind(&slug)
    .fetch_optional(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let new_title = body.title.as_deref().unwrap_or(&existing.title);
    let new_slug = if body.title.is_some() {
        ensure_unique_slug(&db, &slugify(new_title), Some(&existing.id)).await
    } else {
        existing.slug.clone()
    };
    let new_content = body.content.as_deref().unwrap_or(&existing.content);

    sqlx::query(
        "UPDATE notes SET title = ?, slug = ?, content = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(new_title)
    .bind(&new_slug)
    .bind(new_content)
    .bind(&existing.id)
    .execute(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    set_note_tags(&db, &existing.id, body.tags).await;
    update_wikilinks(&db, &existing.id, new_content).await;

    Ok(Json(json!({ "slug": new_slug })))
}

pub async fn delete_note(
    State(db): State<SqlitePool>,
    Path(slug): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let result = sqlx::query("DELETE FROM notes WHERE slug = ?")
        .bind(&slug)
        .execute(&db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::SqlitePool;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:")
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
            "CREATE TABLE tags (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE note_tags (
                note_id TEXT NOT NULL,
                tag_id TEXT NOT NULL,
                PRIMARY KEY (note_id, tag_id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "CREATE TABLE note_links (
                source_note_id TEXT NOT NULL,
                target_note_id TEXT,
                PRIMARY KEY (source_note_id, target_note_id)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();

        pool
    }

    #[tokio::test]
    async fn test_create_and_get_note() {
        let db = setup_db().await;

        let create_body = CreateNote {
            title: "Test Note".into(),
            content: Some("Hello **world**".into()),
            tags: Some(vec!["demo".into()]),
        };

        let (status, slug_json) = create_note(State(db.clone()), Json(create_body))
            .await
            .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        let slug = slug_json.get("slug").unwrap().as_str().unwrap().to_string();

        let note = get_note(State(db), Path(slug)).await.unwrap();
        assert_eq!(note.title, "Test Note");
        assert_eq!(note.content, "Hello **world**");
        assert_eq!(note.tags, vec!["demo"]);
    }

    #[tokio::test]
    async fn test_update_note() {
        let db = setup_db().await;

        // Create
        let create = CreateNote {
            title: "Original".into(),
            content: None,
            tags: None,
        };
        let (_, slug_json) = create_note(State(db.clone()), Json(create))
            .await
            .unwrap();
        let slug = slug_json.get("slug").unwrap().as_str().unwrap().to_string();

        // Update
        let update = UpdateNote {
            title: Some("Updated".into()),
            content: Some("New content".into()),
            tags: Some(vec!["updated".into()]),
        };
        
        // NOTE: Since updating the title automatically updates the note's slug in the backend,
        // we need to capture the newly generated slug from the response.
        let updated_slug_json = update_note(State(db.clone()), Path(slug), Json(update))
            .await
            .unwrap();

        let new_slug = updated_slug_json.get("slug").unwrap().as_str().unwrap().to_string();

        // Verify
        // We use the 'new_slug' here because the old one ("original") no longer exists.
        let note = get_note(State(db), Path(new_slug)).await.unwrap();
        assert_eq!(note.title, "Updated");
        assert_eq!(note.content, "New content");
        assert_eq!(note.tags, vec!["updated"]);
    }

    #[tokio::test]
    async fn test_delete_note() {
        let db = setup_db().await;

        let create = CreateNote {
            title: "To Delete".into(),
            content: None,
            tags: None,
        };
        let (_, slug_json) = create_note(State(db.clone()), Json(create))
            .await
            .unwrap();
        let slug = slug_json.get("slug").unwrap().as_str().unwrap().to_string();

        delete_note(State(db.clone()), Path(slug.clone()))
            .await
            .unwrap();

        let result = get_note(State(db), Path(slug)).await;
        assert!(result.is_err());
    }
}