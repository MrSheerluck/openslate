use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, SqlitePool};
use uuid::Uuid;

use crate::AppState;
use s3::Client;

fn storage(state: &AppState) -> Result<(&Client, &str), StatusCode> {
    match (&state.client, &state.bucket) {
        (Some(client), Some(bucket)) => Ok((client, bucket.as_str())),
        _ => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MediaRow {
    id: String,
    filename: String,
    original_name: String,
    mime_type: String,
    size: i64,
    note_id: Option<String>,
    created_at: String,
}

#[derive(Serialize)]
pub struct MediaItem {
    id: String,
    filename: String,
    original_name: String,
    mime_type: String,
    size: i64,
    note_id: Option<String>,
    tags: Vec<String>,
    created_at: String,
}

#[derive(Deserialize)]
pub struct ListMediaParams {
    r#type: Option<String>,
    note_id: Option<String>,
    q: Option<String>,
    #[serde(rename = "tags")]
    _tags: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateMediaBody {
    tags: Option<Vec<String>>,
    note_id: Option<String>,
}

#[derive(Deserialize)]
pub struct ImportUrlBody {
    url: String,
    note_id: Option<String>,
    tags: Option<String>,
}

fn ext_from_mime(mime: &str) -> &str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/avif" => "avif",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "application/pdf" => "pdf",
        "text/plain" => "txt",
        "text/csv" => "csv",
        "application/json" => "json",
        "application/zip" => "zip",
        "application/x-tar" => "tar",
        "application/gzip" => "gz",
        "application/x-7z-compressed" => "7z",
        _ => "bin",
    }
}

async fn get_media_tags(pool: &SqlitePool, media_id: &str) -> Vec<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT t.name FROM tags t
         JOIN media_tags mt ON mt.tag_id = t.id
         WHERE mt.media_id = ? ORDER BY t.name",
    )
    .bind(media_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

async fn set_media_tags(pool: &SqlitePool, media_id: &str, tags: Option<Vec<String>>) {
    let Some(tags) = tags else { return };

    sqlx::query("DELETE FROM media_tags WHERE media_id = ?")
        .bind(media_id)
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
            sqlx::query("INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?, ?)")
                .bind(media_id)
                .bind(tag_id)
                .execute(pool)
                .await
                .ok();
        }
    }
}

pub async fn upload_media(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<serde_json::Value>), StatusCode> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut mime_type = String::new();
    let mut original_name = String::new();
    let mut note_id: Option<String> = None;
    let mut tags: Option<Vec<String>> = None;

    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                mime_type = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                original_name = field.file_name().unwrap_or("untitled").to_string();
                file_bytes = Some(field.bytes().await.unwrap_or_default().to_vec());
            }
            "note_id" => {
                note_id = Some(field.text().await.unwrap_or_default());
            }
            "tags" => {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() {
                    tags = Some(
                        text.split(',')
                            .map(|t| t.trim().to_string())
                            .filter(|t| !t.is_empty())
                            .collect(),
                    );
                }
            }
            _ => {}
        }
    }

    let bytes = file_bytes.ok_or(StatusCode::BAD_REQUEST)?;

    if bytes.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let id = Uuid::new_v4().to_string();
    let ext = ext_from_mime(&mime_type);
    let filename = format!("{}.{}", id, ext);
    let key = format!("media/{}", filename);

    let (client, bucket) = storage(&state)?;

    client
        .objects()
        .put(bucket, &key)
        .content_type(&mime_type)
        .body_bytes(bytes.clone())
        .send()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query(
        "INSERT INTO media (id, filename, original_name, mime_type, size, note_id) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&filename)
    .bind(&original_name)
    .bind(&mime_type)
    .bind(bytes.len() as i64)
    .bind(&note_id)
    .execute(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    set_media_tags(&state.db, &id, tags).await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "filename": filename })),
    ))
}

pub async fn list_media(
    State(state): State<AppState>,
    Query(params): Query<ListMediaParams>,
) -> Result<Json<Vec<MediaItem>>, StatusCode> {
    let mut builder: QueryBuilder<sqlx::Sqlite> = QueryBuilder::new(
        "SELECT m.id, m.filename, m.original_name, m.mime_type, m.size, m.note_id, m.created_at FROM media m",
    );

    let mut has_where = false;
    if let Some(ref type_filter) = params.r#type {
        builder.push(" WHERE m.mime_type LIKE ");
        builder.push_bind(format!("{}%", type_filter));
        has_where = true;
    }
    if let Some(ref note_id) = params.note_id {
        if has_where {
            builder.push(" AND m.note_id = ");
        } else {
            builder.push(" WHERE m.note_id = ");
            has_where = true;
        }
        builder.push_bind(note_id);
    }
    if let Some(ref q) = params.q
        && !q.is_empty()
    {
        if has_where {
            builder.push(" AND m.original_name LIKE ");
        } else {
            builder.push(" WHERE m.original_name LIKE ");
        }
        builder.push_bind(format!("%{}%", q));
    }

    builder.push(" ORDER BY m.created_at DESC LIMIT 50");

    let rows: Vec<MediaRow> = builder
        .build_query_as()
        .fetch_all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut items = Vec::new();
    for row in rows {
        let tags = get_media_tags(&state.db, &row.id).await;
        items.push(MediaItem {
            id: row.id,
            filename: row.filename,
            original_name: row.original_name,
            mime_type: row.mime_type,
            size: row.size,
            note_id: row.note_id,
            tags,
            created_at: row.created_at,
        });
    }

    Ok(Json(items))
}

pub async fn get_media(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<MediaItem>, StatusCode> {
    let row = sqlx::query_as::<_, MediaRow>(
        "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let tags = get_media_tags(&state.db, &row.id).await;

    Ok(Json(MediaItem {
        id: row.id,
        filename: row.filename,
        original_name: row.original_name,
        mime_type: row.mime_type,
        size: row.size,
        note_id: row.note_id,
        tags,
        created_at: row.created_at,
    }))
}

pub async fn serve_media_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, StatusCode> {
    let row = sqlx::query_as::<_, MediaRow>(
        "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let key = format!("media/{}", row.filename);
    let (client, bucket) = storage(&state)?;

    let result = client
        .objects()
        .get(bucket, &key)
        .send()
        .await
        .map_err(|_| StatusCode::NOT_FOUND)?;

    let bytes = result
        .bytes()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, &row.mime_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", row.original_name),
        )
        .body(axum::body::Body::from(bytes))
        .unwrap())
}

pub async fn delete_media(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, StatusCode> {
    let row = sqlx::query_as::<_, MediaRow>(
        "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    sqlx::query("DELETE FROM media WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let key = format!("media/{}", row.filename);
    let (client, bucket) = storage(&state)?;

    client.objects().delete(bucket, &key).send().await.ok();

    Ok(StatusCode::NO_CONTENT)
}

pub async fn update_media(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMediaBody>,
) -> Result<Json<MediaItem>, StatusCode> {
    let existing = sqlx::query_as::<_, MediaRow>(
        "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(ref note_id) = body.note_id {
        let new_note_id = if note_id.is_empty() {
            None
        } else {
            Some(note_id.clone())
        };
        sqlx::query("UPDATE media SET note_id = ? WHERE id = ?")
            .bind(&new_note_id)
            .bind(&id)
            .execute(&state.db)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    set_media_tags(&state.db, &id, body.tags).await;

    let tags = get_media_tags(&state.db, &id).await;

    let note_id = body.note_id.filter(|n| !n.is_empty()).or(existing.note_id);

    Ok(Json(MediaItem {
        id: existing.id,
        filename: existing.filename,
        original_name: existing.original_name,
        mime_type: existing.mime_type,
        size: existing.size,
        note_id,
        tags,
        created_at: existing.created_at,
    }))
}

fn filename_from_url(url: &str) -> String {
    url.split('/')
        .rfind(|s| !s.is_empty())
        .unwrap_or("untitled")
        .split('?')
        .next()
        .unwrap_or("untitled")
        .to_string()
}

pub async fn import_from_url(
    State(state): State<AppState>,
    Json(body): Json<ImportUrlBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<serde_json::Value>)> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("openslate/1.0")
        .build()
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to create HTTP client"})),
            )
        })?;

    let response = client.get(&body.url).send().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Failed to fetch URL: {}", e)})),
        )
    })?;

    if !response.status().is_success() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("URL returned HTTP {}", response.status())})),
        ));
    }

    let mime_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let bytes = response.bytes().await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Failed to read response body"})),
        )
    })?;

    if bytes.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Empty response body"})),
        ));
    }

    let original_name = filename_from_url(&body.url);
    let id = Uuid::new_v4().to_string();
    let ext = ext_from_mime(&mime_type);
    let filename = format!("{}.{}", id, ext);
    let key = format!("media/{}", filename);

    let (client, bucket) = storage(&state).map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Media storage not configured"})),
        )
    })?;

    client
        .objects()
        .put(bucket, &key)
        .content_type(&mime_type)
        .body_bytes(bytes.to_vec())
        .send()
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to upload file to storage"})),
            )
        })?;

    let note_id = body.note_id.filter(|n| !n.is_empty());

    sqlx::query(
        "INSERT INTO media (id, filename, original_name, mime_type, size, note_id) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&filename)
    .bind(&original_name)
    .bind(&mime_type)
    .bind(bytes.len() as i64)
    .bind(&note_id)
    .execute(&state.db)
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Database error"})),
        )
    })?;

    let tags = body.tags.map(|t| {
        t.split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
    });
    set_media_tags(&state.db, &id, tags).await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": id, "filename": filename })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn setup_media_db() -> sqlx::SqlitePool {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }

    #[tokio::test]
    async fn test_list_media_empty() {
        let db = setup_media_db().await;
        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };

        let Json(items) = list_media(
            State(state),
            Query(ListMediaParams {
                r#type: None,
                note_id: None,
                q: None,
                _tags: None,
            }),
        )
        .await
        .unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn test_list_media_filter_by_type() {
        let db = setup_media_db().await;

        sqlx::query(
            "INSERT INTO media (id, filename, original_name, mime_type, size) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("img1")
        .bind("img1.png")
        .bind("photo.png")
        .bind("image/png")
        .bind(100i64)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO media (id, filename, original_name, mime_type, size) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("vid1")
        .bind("vid1.mp4")
        .bind("video.mp4")
        .bind("video/mp4")
        .bind(200i64)
        .execute(&db)
        .await
        .unwrap();

        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };
        let Json(items) = list_media(
            State(state),
            Query(ListMediaParams {
                r#type: Some("image/".into()),
                note_id: None,
                q: None,
                _tags: None,
            }),
        )
        .await
        .unwrap();

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].mime_type, "image/png");
    }

    #[tokio::test]
    async fn test_get_media_not_found() {
        let db = setup_media_db().await;
        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };

        let result = get_media(State(state), Path("nonexistent".into())).await;
        assert_eq!(result.err().unwrap(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_list_media_filter_by_note_id() {
        let db = setup_media_db().await;
        sqlx::query("INSERT INTO notes (id, title, slug, content) VALUES ('note-1', 'Note 1', 'note-1', '')")
            .execute(&db).await.unwrap();
        sqlx::query("INSERT INTO notes (id, title, slug, content) VALUES ('note-2', 'Note 2', 'note-2', '')")
            .execute(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO media (id, filename, original_name, mime_type, size, note_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("m1").bind("f1.png").bind("f1.png").bind("image/png").bind(100i64).bind("note-1")
        .execute(&db).await.unwrap();
        sqlx::query(
            "INSERT INTO media (id, filename, original_name, mime_type, size, note_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("m2").bind("f2.png").bind("f2.png").bind("image/png").bind(200i64).bind("note-2")
        .execute(&db).await.unwrap();

        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };
        let Json(items) = list_media(
            State(state),
            Query(ListMediaParams {
                r#type: None,
                note_id: Some("note-1".into()),
                q: None,
                _tags: None,
            }),
        )
        .await
        .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "m1");
    }

    #[test]
    fn test_ext_from_mime_jpeg() {
        assert_eq!(ext_from_mime("image/jpeg"), "jpg");
    }

    #[test]
    fn test_ext_from_mime_png() {
        assert_eq!(ext_from_mime("image/png"), "png");
    }

    #[test]
    fn test_ext_from_mime_gif() {
        assert_eq!(ext_from_mime("image/gif"), "gif");
    }

    #[test]
    fn test_ext_from_mime_webp() {
        assert_eq!(ext_from_mime("image/webp"), "webp");
    }

    #[test]
    fn test_ext_from_mime_svg() {
        assert_eq!(ext_from_mime("image/svg+xml"), "svg");
    }

    #[test]
    fn test_ext_from_mime_mp4() {
        assert_eq!(ext_from_mime("video/mp4"), "mp4");
    }

    #[test]
    fn test_ext_from_mime_pdf() {
        assert_eq!(ext_from_mime("application/pdf"), "pdf");
    }

    #[test]
    fn test_ext_from_mime_unknown() {
        assert_eq!(ext_from_mime("application/x-custom"), "bin");
    }

    #[test]
    fn test_filename_from_url_simple() {
        assert_eq!(
            filename_from_url("https://example.com/photo.jpg"),
            "photo.jpg"
        );
    }

    #[test]
    fn test_filename_from_url_with_query_string() {
        assert_eq!(
            filename_from_url("https://example.com/photo.jpg?w=800&h=600"),
            "photo.jpg"
        );
    }

    #[test]
    fn test_filename_from_url_trailing_slash_falls_back_to_host() {
        // With a trailing slash the last non-empty segment is the host.
        assert_eq!(filename_from_url("https://example.com/"), "example.com");
    }

    #[test]
    fn test_filename_from_url_without_filename_returns_untitled() {
        // No usable path segment at all -> the "untitled" fallback kicks in.
        assert_eq!(filename_from_url(""), "untitled");
    }
}
