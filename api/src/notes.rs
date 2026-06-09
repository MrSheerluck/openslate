use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::db::{self, Database, DbParam, FromRow, Row};

#[derive(Serialize)]
struct NoteRow {
    id: String,
    title: String,
    slug: String,
    content: String,
    created_at: String,
    updated_at: String,
}

impl FromRow for NoteRow {
    fn from_row(row: &Row) -> db::DbResult<Self> {
        Ok(Self {
            id: row.str("id")?,
            title: row.str("title")?,
            slug: row.str("slug")?,
            content: row.str("content")?,
            created_at: row.str("created_at")?,
            updated_at: row.str("updated_at")?,
        })
    }
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

#[derive(Serialize)]
pub struct LinkInfo {
    pub title: String,
    pub slug: String,
}

impl FromRow for LinkInfo {
    fn from_row(row: &Row) -> db::DbResult<Self> {
        Ok(Self {
            title: row.str("title")?,
            slug: row.str("slug")?,
        })
    }
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

async fn ensure_unique_slug(db: &Database, slug: &str, exclude_id: Option<&str>) -> String {
    let mut candidate = slug.to_string();
    let mut counter = 1;
    loop {
        let exists = if let Some(id) = exclude_id {
            db.scalar_i64(
                "SELECT COUNT(*) FROM notes WHERE slug = ?1 AND id != ?2",
                &[candidate.as_str().into(), DbParam::Text(id.to_string())],
            )
            .await
            .unwrap_or(0)
                > 0
        } else {
            db.scalar_i64(
                "SELECT COUNT(*) FROM notes WHERE slug = ?",
                &[candidate.as_str().into()],
            )
            .await
            .unwrap_or(0)
                > 0
        };
        if !exists {
            return candidate;
        }
        candidate = format!("{}-{}", slug, counter);
        counter += 1;
    }
}

async fn get_note_tags(db: &Database, note_id: &str) -> Vec<String> {
    db.scalar_str_all(
        "SELECT t.name FROM tags t
         JOIN note_tags nt ON nt.tag_id = t.id
         WHERE nt.note_id = ? ORDER BY t.name",
        &[note_id.into()],
    )
    .await
    .unwrap_or_default()
}

async fn get_backlinks(db: &Database, note_id: &str) -> Vec<LinkInfo> {
    db.row_all::<LinkInfo>(
        "SELECT n.title, n.slug FROM notes n
         JOIN note_links nl ON nl.source_note_id = n.id
         WHERE nl.target_note_id = ? ORDER BY n.title",
        &[note_id.into()],
    )
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

async fn update_wikilinks(db: &Database, note_id: &str, content: &str) {
    db.execute(
        "DELETE FROM note_links WHERE source_note_id = ?",
        &[note_id.into()],
    )
    .await
    .ok();

    for slug in &parse_wikilinks(content) {
        let target_id: Option<String> = db
            .scalar_str_opt(
                "SELECT id FROM notes WHERE slug = ?",
                &[slug.as_str().into()],
            )
            .await
            .unwrap_or(None);

        db.execute(
            "INSERT OR IGNORE INTO note_links (source_note_id, target_note_id) VALUES (?, ?)",
            &[note_id.into(), target_id.into()],
        )
        .await
        .ok();
    }
}

async fn set_note_tags(db: &Database, note_id: &str, tags: Option<Vec<String>>) {
    let Some(tags) = tags else { return };

    db.execute("DELETE FROM note_tags WHERE note_id = ?", &[note_id.into()])
        .await
        .ok();

    for name in &tags {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        db.execute(
            "INSERT OR IGNORE INTO tags (id, name) VALUES (?, ?)",
            &[Uuid::new_v4().to_string().into(), name.into()],
        )
        .await
        .ok();

        if let Some(tag_id) = db
            .scalar_str_opt("SELECT id FROM tags WHERE name = ?", &[name.into()])
            .await
            .unwrap_or(None)
        {
            db.execute(
                "INSERT OR IGNORE INTO note_tags (note_id, tag_id) VALUES (?, ?)",
                &[note_id.into(), tag_id.into()],
            )
            .await
            .ok();
        }
    }
}

pub async fn list_notes(
    State(state): State<AppState>,
) -> Result<Json<Vec<NoteSummary>>, StatusCode> {
    let notes = state
        .db
        .row_all::<NoteRow>(
            "SELECT id, title, slug, content, created_at, updated_at FROM notes ORDER BY updated_at DESC",
            &[],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut result = Vec::new();
    for note in notes {
        let tags = get_note_tags(&state.db, &note.id).await;
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
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<Json<NoteResponse>, StatusCode> {
    let note = state
        .db
        .row_opt::<NoteRow>(
            "SELECT id, title, slug, content, created_at, updated_at FROM notes WHERE slug = ?",
            &[slug.as_str().into()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let tags = get_note_tags(&state.db, &note.id).await;
    let backlinks = get_backlinks(&state.db, &note.id).await;

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
    State(state): State<AppState>,
    Json(body): Json<CreateNote>,
) -> Result<(StatusCode, Json<Value>), StatusCode> {
    let id = Uuid::new_v4().to_string();
    let slug = slugify(&body.title);
    let unique_slug = ensure_unique_slug(&state.db, &slug, None).await;
    let content = body.content.unwrap_or_default();

    state
        .db
        .execute(
            "INSERT INTO notes (id, title, slug, content) VALUES (?, ?, ?, ?)",
            &[
                id.as_str().into(),
                body.title.as_str().into(),
                unique_slug.as_str().into(),
                content.as_str().into(),
            ],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    set_note_tags(&state.db, &id, body.tags).await;
    update_wikilinks(&state.db, &id, &content).await;

    Ok((StatusCode::CREATED, Json(json!({ "slug": unique_slug }))))
}

pub async fn update_note(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Json(body): Json<UpdateNote>,
) -> Result<Json<Value>, StatusCode> {
    let existing = state
        .db
        .row_opt::<NoteRow>(
            "SELECT id, title, slug, content, created_at, updated_at FROM notes WHERE slug = ?",
            &[slug.as_str().into()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let new_title = body.title.as_deref().unwrap_or(&existing.title);
    let new_slug = if body.title.is_some() {
        ensure_unique_slug(&state.db, &slugify(new_title), Some(&existing.id)).await
    } else {
        existing.slug.clone()
    };
    let new_content = body.content.as_deref().unwrap_or(&existing.content);

    state
        .db
        .execute(
            "UPDATE notes SET title = ?, slug = ?, content = ?, updated_at = datetime('now') WHERE id = ?",
            &[
                new_title.into(),
                new_slug.as_str().into(),
                new_content.into(),
                existing.id.as_str().into(),
            ],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    set_note_tags(&state.db, &existing.id, body.tags).await;
    update_wikilinks(&state.db, &existing.id, new_content).await;

    Ok(Json(json!({ "slug": new_slug })))
}

pub async fn delete_note(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let rows = state
        .db
        .execute("DELETE FROM notes WHERE slug = ?", &[slug.as_str().into()])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if rows == 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}
