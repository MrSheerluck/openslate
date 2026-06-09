use axum::{
    Json,
    extract::{Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AppState;
use crate::db::{self, Database, DbParam, FromRow, Row};
use s3::Client;

fn storage(state: &AppState) -> Result<(&Client, &str), StatusCode> {
    match (&state.client, &state.bucket) {
        (Some(client), Some(bucket)) => Ok((client, bucket.as_str())),
        _ => Err(StatusCode::SERVICE_UNAVAILABLE),
    }
}

#[derive(Serialize)]
pub struct MediaRow {
    id: String,
    filename: String,
    original_name: String,
    mime_type: String,
    size: i64,
    note_id: Option<String>,
    created_at: String,
}

impl FromRow for MediaRow {
    fn from_row(row: &Row) -> db::DbResult<Self> {
        Ok(Self {
            id: row.str("id")?,
            filename: row.str("filename")?,
            original_name: row.str("original_name")?,
            mime_type: row.str("mime_type")?,
            size: row.i64("size")?,
            note_id: row.opt_str("note_id")?,
            created_at: row.str("created_at")?,
        })
    }
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

async fn get_media_tags(db: &Database, media_id: &str) -> Vec<String> {
    db.scalar_str_all(
        "SELECT t.name FROM tags t
         JOIN media_tags mt ON mt.tag_id = t.id
         WHERE mt.media_id = ? ORDER BY t.name",
        &[media_id.into()],
    )
    .await
    .unwrap_or_default()
}

async fn set_media_tags(db: &Database, media_id: &str, tags: Option<Vec<String>>) {
    let Some(tags) = tags else { return };

    db.execute(
        "DELETE FROM media_tags WHERE media_id = ?",
        &[media_id.into()],
    )
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
                "INSERT OR IGNORE INTO media_tags (media_id, tag_id) VALUES (?, ?)",
                &[media_id.into(), tag_id.into()],
            )
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

    state
        .db
        .execute(
            "INSERT INTO media (id, filename, original_name, mime_type, size, note_id) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                id.as_str().into(),
                filename.as_str().into(),
                original_name.as_str().into(),
                mime_type.as_str().into(),
                (bytes.len() as i64).into(),
                note_id.into(),
            ],
        )
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
    let mut sql = String::from(
        "SELECT m.id, m.filename, m.original_name, m.mime_type, m.size, m.note_id, m.created_at FROM media m",
    );
    let mut db_params: Vec<DbParam> = Vec::new();

    if let Some(ref type_filter) = params.r#type {
        sql.push_str(" WHERE m.mime_type LIKE ?");
        db_params.push(format!("{}%", type_filter).into());
    }

    if let Some(ref note_id) = params.note_id {
        if db_params.is_empty() {
            sql.push_str(" WHERE m.note_id = ?");
        } else {
            sql.push_str(" AND m.note_id = ?");
        }
        db_params.push(note_id.as_str().into());
    }

    if let Some(ref q) = params.q {
        if !q.is_empty() {
            if db_params.is_empty() {
                sql.push_str(" WHERE m.original_name LIKE ?");
            } else {
                sql.push_str(" AND m.original_name LIKE ?");
            }
            db_params.push(format!("%{}%", q).into());
        }
    }

    sql.push_str(" ORDER BY m.created_at DESC LIMIT 50");

    let rows = state
        .db
        .row_all::<MediaRow>(&sql, &db_params)
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
    let row = state
        .db
        .row_opt::<MediaRow>(
            "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
            &[id.as_str().into()],
        )
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
    let row = state
        .db
        .row_opt::<MediaRow>(
            "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
            &[id.as_str().into()],
        )
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
    let row = state
        .db
        .row_opt::<MediaRow>(
            "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
            &[id.as_str().into()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    state
        .db
        .execute("DELETE FROM media WHERE id = ?", &[id.as_str().into()])
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
    let existing = state
        .db
        .row_opt::<MediaRow>(
            "SELECT id, filename, original_name, mime_type, size, note_id, created_at FROM media WHERE id = ?",
            &[id.as_str().into()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Some(ref note_id) = body.note_id {
        let new_note_id: DbParam = if note_id.is_empty() {
            DbParam::Null
        } else {
            note_id.as_str().into()
        };
        state
            .db
            .execute(
                "UPDATE media SET note_id = ? WHERE id = ?",
                &[new_note_id, id.as_str().into()],
            )
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

    state
        .db
        .execute(
            "INSERT INTO media (id, filename, original_name, mime_type, size, note_id) VALUES (?, ?, ?, ?, ?, ?)",
            &[
                id.as_str().into(),
                filename.as_str().into(),
                original_name.as_str().into(),
                mime_type.as_str().into(),
                (bytes.len() as i64).into(),
                note_id.into(),
            ],
        )
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
