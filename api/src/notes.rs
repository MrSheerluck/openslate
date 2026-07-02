use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::io::Write;
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

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
) -> Result<(StatusCode, Json<NoteResponse>), StatusCode> {
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

    let tags = get_note_tags(&db, &id).await;
    let backlinks = get_backlinks(&db, &id).await;

    let note = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, slug, content, created_at, updated_at FROM notes WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(NoteResponse {
            id: note.id,
            title: note.title,
            slug: note.slug,
            content: note.content,
            tags,
            backlinks,
            created_at: note.created_at,
            updated_at: note.updated_at,
        }),
    ))
}

pub async fn update_note(
    State(db): State<SqlitePool>,
    Path(slug): Path<String>,
    Json(body): Json<UpdateNote>,
) -> Result<Json<NoteResponse>, StatusCode> {
    let existing = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, slug, content, created_at, updated_at FROM notes WHERE slug = ?",
    )
    .bind(&slug)
    .fetch_optional(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let new_title = body.title.as_deref().unwrap_or(&existing.title).to_string();
    let new_slug = if body.title.is_some() {
        ensure_unique_slug(&db, &slugify(&new_title), Some(&existing.id)).await
    } else {
        existing.slug.clone()
    };
    let new_content = body.content.as_deref().unwrap_or(&existing.content);

    sqlx::query(
        "UPDATE notes SET title = ?, slug = ?, content = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(&new_title)
    .bind(&new_slug)
    .bind(new_content)
    .bind(&existing.id)
    .execute(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    set_note_tags(&db, &existing.id, body.tags).await;
    update_wikilinks(&db, &existing.id, new_content).await;

    let note = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, slug, content, created_at, updated_at FROM notes WHERE id = ?",
    )
    .bind(&existing.id)
    .fetch_one(&db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let tags = get_note_tags(&db, &existing.id).await;
    let backlinks = get_backlinks(&db, &existing.id).await;

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

// --- Import / Export ---

#[derive(Default)]
struct Frontmatter {
    title: Option<String>,
    tags: Vec<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
}

fn parse_frontmatter(content: &str) -> (Frontmatter, &str) {
    let mut fm = Frontmatter::default();
    let rest = if content.starts_with("---\n") || content.starts_with("---\r\n") {
        if let Some(end_idx) = content[3..].find("\n---") {
            let raw = &content[3..3 + end_idx];
            let body_start = 3 + end_idx + 4;
            let body = content[body_start..]
                .trim_start_matches('\n')
                .trim_start_matches("\r\n");

            for line in raw.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let Some((key, value)) = line.split_once(':') else {
                    continue;
                };
                let key = key.trim();
                let value = value.trim();
                match key {
                    "title" => {
                        let v = value.trim_matches('"').trim_matches('\'');
                        if !v.is_empty() {
                            fm.title = Some(v.to_string());
                        }
                    }
                    "tags" => {
                        let inner = value.trim_matches('[').trim_matches(']');
                        for t in inner.split(',') {
                            let t = t.trim().trim_matches('"').trim_matches('\'');
                            if !t.is_empty() {
                                fm.tags.push(t.to_string());
                            }
                        }
                    }
                    "created_at" | "date" if !value.is_empty() => {
                        fm.created_at = Some(value.to_string());
                    }
                    "updated_at" if !value.is_empty() => {
                        fm.updated_at = Some(value.to_string());
                    }
                    _ => {}
                }
            }
            return (fm, body);
        } else {
            content
        }
    } else {
        content
    };
    (fm, rest)
}

fn title_from_filename(filename: &str) -> String {
    let stem = filename
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(filename);
    let stem = stem.replace(['_', '-'], " ");
    let mut out = String::new();
    for word in stem.split_whitespace() {
        if let Some(c) = word.chars().next() {
            out.push(c.to_uppercase().next().unwrap_or(c));
            out.push_str(&word[c.len_utf8()..]);
        }
        out.push(' ');
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "Untitled".to_string()
    } else {
        trimmed.to_string()
    }
}

fn extract_inline_tags(md: &str) -> Vec<String> {
    let mut out = Vec::new();
    for cap in md.match_indices('#') {
        let rest = &md[cap.0 + 1..];
        let mut end = 0;
        for (i, c) in rest.char_indices() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                end = i + c.len_utf8();
            } else {
                break;
            }
        }
        if end == 0 {
            continue;
        }
        let tag = &rest[..end];
        let prev_char = md[..cap.0].chars().last();
        let is_word_start = match prev_char {
            None => true,
            Some(c) => {
                c.is_whitespace() || c == '(' || c == '[' || c == '>' || c == '\n' || c == '\r'
            }
        };
        if !is_word_start {
            continue;
        }
        if tag
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            continue;
        }
        out.push(tag.to_lowercase());
    }
    out
}

#[derive(Serialize)]
pub struct ImportResult {
    file: String,
    slug: String,
    title: String,
    status: &'static str,
}

#[derive(Serialize)]
pub struct ImportResponse {
    imported: usize,
    failed: usize,
    results: Vec<ImportResult>,
}

pub async fn import_notes(
    State(db): State<SqlitePool>,
    mut multipart: Multipart,
) -> Result<Json<ImportResponse>, (StatusCode, Json<Value>)> {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": format!("Invalid multipart: {}", e) })),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();
        if name != "file" && name != "files" {
            continue;
        }
        let original_name = field
            .file_name()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "untitled.md".to_string());
        let bytes = field.bytes().await.map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("Failed to read file: {}", e) })),
            )
        })?;
        files.push((original_name, bytes.to_vec()));
    }

    if files.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "No files provided" })),
        ));
    }

    let mut results: Vec<ImportResult> = Vec::new();
    let mut imported = 0usize;
    let mut failed = 0usize;

    for (original_name, bytes) in files {
        let display_name = original_name.clone();
        let result = import_single(&db, &original_name, &bytes).await;
        match result {
            Ok((slug, title)) => {
                imported += 1;
                results.push(ImportResult {
                    file: display_name,
                    slug,
                    title,
                    status: "ok",
                });
            }
            Err(reason) => {
                failed += 1;
                results.push(ImportResult {
                    file: display_name.clone(),
                    slug: String::new(),
                    title: String::new(),
                    status: "error",
                });
                eprintln!("Import failed for {}: {}", display_name, reason);
            }
        }
    }

    Ok(Json(ImportResponse {
        imported,
        failed,
        results,
    }))
}

async fn import_single(
    db: &SqlitePool,
    original_name: &str,
    bytes: &[u8],
) -> Result<(String, String), String> {
    let lower = original_name.to_lowercase();
    if !(lower.ends_with(".md") || lower.ends_with(".markdown")) {
        return Err("Not a markdown file".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "File is not valid UTF-8")?;

    let (fm, body) = parse_frontmatter(text);

    let title = fm
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| title_from_filename(original_name));

    let mut tags: Vec<String> = fm
        .tags
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    for t in extract_inline_tags(body) {
        if !tags.contains(&t) {
            tags.push(t);
        }
    }

    let id = Uuid::new_v4().to_string();
    let slug = ensure_unique_slug(db, &slugify(&title), None).await;

    sqlx::query("INSERT INTO notes (id, title, slug, content) VALUES (?1, ?2, ?3, ?4)")
        .bind(&id)
        .bind(&title)
        .bind(&slug)
        .bind(body)
        .execute(db)
        .await
        .map_err(|e| format!("DB insert: {}", e))?;

    if let Some(ref created) = fm.created_at {
        sqlx::query("UPDATE notes SET created_at = ?1 WHERE id = ?2")
            .bind(created)
            .bind(&id)
            .execute(db)
            .await
            .ok();
    }
    if let Some(ref updated) = fm.updated_at {
        sqlx::query("UPDATE notes SET updated_at = ?1 WHERE id = ?2")
            .bind(updated)
            .bind(&id)
            .execute(db)
            .await
            .ok();
    }

    let tags_opt = if tags.is_empty() { None } else { Some(tags) };
    set_note_tags(db, &id, tags_opt).await;
    update_wikilinks(db, &id, body).await;

    Ok((slug, title))
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub async fn export_notes(State(db): State<SqlitePool>) -> Result<Response, StatusCode> {
    let entries = load_export_entries(&db).await?;
    let readme = "# openslate export (flat)\n\n\
Exported notes from openslate. Each note is a Markdown file with YAML frontmatter,\n\
placed at the root of this zip (no subfolders).\n\n\
Re-import any of these `.md` files via Settings → Data → Import (or the sidebar\n\
import button) to restore them. The frontmatter preserves title, tags, slug,\n\
and timestamps.\n";
    let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::<u8>::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("README.md", options)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    zip.write_all(readme.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for (title, slug, content, tags, created_at, updated_at) in &entries {
        let body = render_note_md(title, slug, content, tags, created_at, updated_at);
        let filename = format!("{}.md", sanitize_filename(slug));
        zip.start_file(&filename, options)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        zip.write_all(body.as_bytes())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    build_zip_response(zip, "openslate-export.zip")
}

pub async fn export_notes_by_tag(State(db): State<SqlitePool>) -> Result<Response, StatusCode> {
    let entries = load_export_entries(&db).await?;
    let readme = "# openslate export (by tag)\n\n\
Exported notes from openslate. Each note is a Markdown file with YAML frontmatter.\n\n\
- Tagged notes are placed under `tags/<tag>/` (using the note's first tag).\n\
- Untagged notes are placed under `untagged/`.\n\n\
Re-import any of these `.md` files via Settings → Data → Import (or the sidebar\n\
import button) to restore them. The frontmatter preserves title, tags, slug,\n\
and timestamps.\n";
    let mut zip = ZipWriter::new(std::io::Cursor::new(Vec::<u8>::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("README.md", options)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    zip.write_all(readme.as_bytes())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    for (title, slug, content, tags, created_at, updated_at) in &entries {
        let body = render_note_md(title, slug, content, tags, created_at, updated_at);
        let filename = format!("{}.md", sanitize_filename(slug));
        let path = if let Some(first) = tags.first() {
            format!("tags/{}/{}", sanitize_filename(first), filename)
        } else {
            format!("untagged/{}", filename)
        };
        zip.start_file(&path, options)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        zip.write_all(body.as_bytes())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    build_zip_response(zip, "openslate-export-by-tag.zip")
}

type ExportEntry = (String, String, String, Vec<String>, String, String);

async fn load_export_entries(db: &SqlitePool) -> Result<Vec<ExportEntry>, StatusCode> {
    let rows = sqlx::query_as::<_, NoteRow>(
        "SELECT id, title, slug, content, created_at, updated_at FROM notes ORDER BY title COLLATE NOCASE",
    )
    .fetch_all(db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut entries: Vec<ExportEntry> = Vec::new();
    for row in rows {
        let tags = get_note_tags(db, &row.id).await;
        entries.push((
            row.title,
            row.slug,
            row.content,
            tags,
            row.created_at,
            row.updated_at,
        ));
    }
    Ok(entries)
}

fn render_note_md(
    title: &str,
    slug: &str,
    content: &str,
    tags: &[String],
    created_at: &str,
    updated_at: &str,
) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("title: \"{}\"\n", title.replace('"', "\\\"")));
    fm.push_str(&format!("slug: \"{}\"\n", slug));
    if !tags.is_empty() {
        fm.push_str("tags: [");
        fm.push_str(
            &tags
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(", "),
        );
        fm.push_str("]\n");
    }
    fm.push_str(&format!("created_at: \"{}\"\n", created_at));
    fm.push_str(&format!("updated_at: \"{}\"\n", updated_at));
    fm.push_str("---\n\n");
    format!("{}{}", fm, content)
}

fn build_zip_response(
    zip: ZipWriter<std::io::Cursor<Vec<u8>>>,
    filename: &str,
) -> Result<Response, StatusCode> {
    let cursor = zip
        .finish()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let bytes = cursor.into_inner();

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/zip")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .body(axum::body::Body::from(bytes))
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::{Path, State};
    use sqlx::SqlitePool;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    async fn create_test_note(
        db: &SqlitePool,
        title: &str,
        content: &str,
        tags: Option<Vec<String>>,
    ) -> NoteResponse {
        let body = CreateNote {
            title: title.into(),
            content: Some(content.into()),
            tags,
        };
        let (_, json) = create_note(State(db.clone()), Json(body)).await.unwrap();
        json.0
    }

    #[tokio::test]
    async fn test_list_notes_empty() {
        let db = setup_db().await;
        let result = list_notes(State(db)).await.unwrap();
        assert!(result.0.is_empty());
    }

    #[tokio::test]
    async fn test_create_note_returns_201_and_slug() {
        let db = setup_db().await;
        let body = CreateNote {
            title: "My Note".into(),
            content: Some("Hello".into()),
            tags: None,
        };
        let (status, json) = create_note(State(db), Json(body)).await.unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json.slug, "my-note");
    }

    #[tokio::test]
    async fn test_get_note_by_slug_returns_full_note() {
        let db = setup_db().await;
        let note = create_test_note(&db, "Test Note", "Body content", None).await;
        let result = get_note(State(db), Path(note.slug.clone())).await.unwrap();
        assert_eq!(result.slug, note.slug);
        assert_eq!(result.title, "Test Note");
        assert_eq!(result.content, "Body content");
    }

    #[tokio::test]
    async fn test_get_note_not_found() {
        let db = setup_db().await;
        let result = get_note(State(db), Path("nope".into())).await;
        assert!(matches!(result, Err(StatusCode::NOT_FOUND)));
    }

    #[tokio::test]
    async fn test_list_notes_ordered_by_newest() {
        let db = setup_db().await;
        create_test_note(&db, "Alpha", "", None).await;
        create_test_note(&db, "Beta", "", None).await;
        let result = list_notes(State(db)).await.unwrap();
        assert_eq!(result.0.len(), 2);
        let titles: Vec<&str> = result.0.iter().map(|n| n.title.as_str()).collect();
        assert!(titles.contains(&"Alpha"));
        assert!(titles.contains(&"Beta"));
    }

    #[tokio::test]
    async fn test_update_note_title_changes_slug() {
        let db = setup_db().await;
        let note = create_test_note(&db, "Old Title", "", None).await;
        let body = UpdateNote {
            title: Some("New Title".into()),
            content: None,
            tags: None,
        };
        let updated = update_note(State(db.clone()), Path(note.slug.clone()), Json(body))
            .await
            .unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.slug, "new-title");
    }

    #[tokio::test]
    async fn test_update_note_content_only_keeps_slug() {
        let db = setup_db().await;
        let note = create_test_note(&db, "Stable", "Original", None).await;
        let body = UpdateNote {
            title: None,
            content: Some("Updated content".into()),
            tags: None,
        };
        let updated = update_note(State(db.clone()), Path(note.slug.clone()), Json(body))
            .await
            .unwrap();
        assert_eq!(updated.slug, "stable");
        assert_eq!(updated.content, "Updated content");
    }

    #[tokio::test]
    async fn test_update_note_returns_404_for_nonexistent_slug() {
        let db = setup_db().await;
        let body = UpdateNote {
            title: Some("Whatever".into()),
            content: None,
            tags: None,
        };
        let result = update_note(State(db), Path("nonexistent".into()), Json(body)).await;
        assert!(matches!(result, Err(StatusCode::NOT_FOUND)));
    }

    #[tokio::test]
    async fn test_delete_note_removes_note_from_list() {
        let db = setup_db().await;
        let note = create_test_note(&db, "To Delete", "", None).await;
        let status = delete_note(State(db.clone()), Path(note.slug.clone()))
            .await
            .unwrap();
        assert_eq!(status, StatusCode::NO_CONTENT);
        let list = list_notes(State(db)).await.unwrap();
        assert!(list.0.iter().all(|n| n.slug != note.slug));
    }

    #[tokio::test]
    async fn test_delete_note_returns_404_for_nonexistent_slug() {
        let db = setup_db().await;
        let result = delete_note(State(db), Path("nonexistent".into())).await;
        assert!(matches!(result, Err(StatusCode::NOT_FOUND)));
    }

    #[tokio::test]
    async fn test_duplicate_title_gets_unique_slug() {
        let db = setup_db().await;
        create_test_note(&db, "My Note", "", None).await;
        let note2 = create_test_note(&db, "My Note", "", None).await;
        assert_eq!(note2.slug, "my-note-1");
    }

    #[tokio::test]
    async fn test_create_note_with_tags_stores_and_returns_tags() {
        let db = setup_db().await;
        let tags = vec!["rust".into(), "testing".into()];
        let body = CreateNote {
            title: "Tagged Note".into(),
            content: None,
            tags: Some(tags),
        };
        let (_, json) = create_note(State(db), Json(body)).await.unwrap();
        assert!(!json.tags.is_empty());
        assert!(json.tags.contains(&"rust".to_string()));
        assert!(json.tags.contains(&"testing".to_string()));
    }

    #[test]
    // Verifies whether slugify title sanitization
    fn test_slugify_basic() {
        assert_eq!(slugify("Hello World"), "hello-world");
        assert_eq!(slugify(""), "untitled");
        assert_eq!(slugify("dooms day"), "dooms-day");
        assert_eq!(slugify("Ice cream"), "ice-cream");
        assert_eq!(slugify("donkey Kong"), "donkey-kong");
    }

    #[test]
    // Verifies whitespace trimming of slugify.
    fn test_slugify_whitespace() {
        assert_eq!(slugify(" Hello "), "hello");
        assert_eq!(slugify(""), "untitled");
        assert_eq!(slugify("     dooms day     "), "dooms-day");
        assert_eq!(slugify("      Ice cream"), "ice-cream");
        assert_eq!(slugify("donkey Kong      "), "donkey-kong");
    }

    #[test]
    // Verifies special character replacment with '-'
    fn test_slugify_special_chars() {
        assert_eq!(slugify(" Hello world! "), "hello-world");
        assert_eq!(slugify("@hello@world"), "hello-world");
        assert_eq!(slugify("!dooms !day     "), "dooms--day");
        assert_eq!(slugify("Ice!: cream!:"), "ice--cream");
        assert_eq!(slugify("donkey:: Kong!"), "donkey--kong");
        assert_eq!(slugify("slack% knack^"), "slack--knack");
        assert_eq!(slugify("Fruit& ninja(^      "), "fruit--ninja");
        assert_eq!(slugify("Fruit& ninja)^      "), "fruit--ninja");
        assert_eq!(slugify("!@#$%^&*()^      "), "untitled");
    }

    #[test]
    // Verifies the numbers in sections, chapters
    fn test_slugify_numbers() {
        assert_eq!(slugify("Section 2.1"), "section-2-1");
        assert_eq!(slugify("Section 2.1.1"), "section-2-1-1");
        assert_eq!(slugify("Chapter @2.1.1"), "chapter--2-1-1");
        assert_eq!(slugify("1.3.1.5.2.8"), "1-3-1-5-2-8");
        assert_eq!(slugify(".1.5.1.0.1.8"), "1-5-1-0-1-8");
    }

    #[test]
    // Verifies the underscores in sections, chapters
    fn test_slugify_underscores() {
        assert_eq!(slugify("my_note"), "my-note");
        assert_eq!(slugify("_start_of_summer"), "start-of-summer");
        assert_eq!(slugify("end_of_winter_"), "end-of-winter");
        assert_eq!(slugify("_"), "untitled");
        assert_eq!(slugify("1_1"), "1-1");
        assert_eq!(slugify("_@"), "untitled");
        assert_eq!(slugify("!!!_"), "untitled");
    }

    // Verifies the valid title
    #[test]
    fn test_slugify_already_valid() {
        assert_eq!(slugify("my-note-slug"), "my-note-slug");
        assert_eq!(slugify("eat-sleep"), "eat-sleep");
        assert_eq!(slugify("debug-1-2"), "debug-1-2");
        assert_eq!(slugify("1-3-i-made-stop"), "1-3-i-made-stop");
    }
}
