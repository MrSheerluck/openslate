use axum::{Json, http::StatusCode};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use crate::auth;

#[derive(Deserialize)]
pub struct AuthBody {
    pub password: String,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    #[allow(dead_code)]
    id: String,
    password_hash: String,
}

pub async fn user_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}

pub async fn status(state: axum::extract::State<crate::AppState>) -> Json<serde_json::Value> {
    let count = user_count(&state.db).await;
    Json(serde_json::json!({ "has_users": count > 0 }))
}

pub async fn create_first_user(pool: &SqlitePool, password: &str) -> Result<(), StatusCode> {
    let count = user_count(pool).await;
    if count > 0 {
        return Err(StatusCode::CONFLICT);
    }

    let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    sqlx::query("INSERT INTO users (id, username, password_hash) VALUES (?, 'admin', ?)")
        .bind(Uuid::new_v4().to_string())
        .bind(&hash)
        .execute(pool)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(())
}

async fn get_user(pool: &SqlitePool) -> Result<UserRow, StatusCode> {
    sqlx::query_as::<_, UserRow>("SELECT id, password_hash FROM users LIMIT 1")
        .fetch_optional(pool)
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

    sqlx::query("UPDATE users SET password_hash = ?, updated_at = datetime('now')")
        .bind(&hash)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(json!({ "success": true })))
}

#[cfg(test)]
mod tests {
    use axum::extract::State;
    use serial_test::serial;

    use super::*;

    async fn setup_db() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    fn app_state(db: SqlitePool) -> crate::AppState {
        crate::AppState {
            db,
            client: None,
            bucket: None,
        }
    }

    #[tokio::test]
    #[serial]
    async fn test_status_after_signup() {
        unsafe {
            std::env::set_var("JWT_SECRET", "test_secret");
        }
        let db = setup_db().await;
        let state = app_state(db);
        let jar = CookieJar::default();

        let body = Json(AuthBody { password: "pass".into() });
        signup(jar, State(state.clone()), body).await.unwrap();

        let Json(json) = status(State(state)).await;
        assert_eq!(json["has_users"], true);
    }
}
