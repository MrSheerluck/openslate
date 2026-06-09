use axum::{Json, http::StatusCode};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Deserialize;
use serde_json::json;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::auth;
use crate::db::{self, Database, FromRow, Row};

#[derive(Deserialize)]
pub struct AuthBody {
    pub password: String,
}

struct UserRow {
    #[allow(dead_code)]
    id: String,
    password_hash: String,
}

impl FromRow for UserRow {
    fn from_row(row: &Row) -> db::DbResult<Self> {
        Ok(Self {
            id: row.str("id")?,
            password_hash: row.str("password_hash")?,
        })
    }
}

pub async fn user_count(db: &Database) -> i64 {
    db.scalar_i64("SELECT COUNT(*) FROM users", &[])
        .await
        .unwrap_or(0)
}

pub async fn status(state: axum::extract::State<crate::AppState>) -> Json<serde_json::Value> {
    let count = user_count(&state.db).await;
    Json(serde_json::json!({ "has_users": count > 0 }))
}

pub async fn create_first_user(db: &Database, password: &str) -> Result<(), StatusCode> {
    let count = user_count(db).await;
    if count > 0 {
        return Err(StatusCode::CONFLICT);
    }

    let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    db.execute(
        "INSERT INTO users (id, username, password_hash) VALUES (?, 'admin', ?)",
        &[Uuid::new_v4().to_string().into(), hash.into()],
    )
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}

async fn get_user(db: &Database) -> Result<UserRow, StatusCode> {
    db.row_opt::<UserRow>("SELECT id, password_hash FROM users LIMIT 1", &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)
}

fn create_auth_cookie(secret: &str) -> Result<Cookie<'static>, StatusCode> {
    let now = OffsetDateTime::now_utc();
    let exp = now + Duration::days(30);

    let claims = auth::Claims {
        sub: "admin".into(),
        exp: exp.unix_timestamp() as usize,
        iat: now.unix_timestamp() as usize,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Cookie::build(("token", token))
        .path("/")
        .http_only(true)
        .secure(false)
        .same_site(axum_extra::extract::cookie::SameSite::Lax)
        .max_age(Duration::days(30))
        .build())
}

pub async fn signup(
    jar: CookieJar,
    state: axum::extract::State<crate::AppState>,
    Json(body): Json<AuthBody>,
) -> Result<(CookieJar, Json<serde_json::Value>), StatusCode> {
    create_first_user(&state.db, &body.password).await?;
    let cookie = create_auth_cookie(&auth::jwt_secret())?;
    Ok((jar.add(cookie), Json(json!({ "success": true }))))
}

pub async fn signin(
    jar: CookieJar,
    state: axum::extract::State<crate::AppState>,
    Json(body): Json<AuthBody>,
) -> Result<(CookieJar, Json<serde_json::Value>), StatusCode> {
    let user = get_user(&state.db).await?;

    let valid = bcrypt::verify(&body.password, &user.password_hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let cookie = create_auth_cookie(&auth::jwt_secret())?;
    Ok((jar.add(cookie), Json(json!({ "success": true }))))
}

pub async fn change_password(
    state: axum::extract::State<crate::AppState>,
    Json(body): Json<AuthBody>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let hash = bcrypt::hash(&body.password, bcrypt::DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    state
        .db
        .execute(
            "UPDATE users SET password_hash = ?, updated_at = datetime('now')",
            &[hash.into()],
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "success": true })))
}
