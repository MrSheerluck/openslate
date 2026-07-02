use axum::{Json, extract::Request, http::StatusCode, middleware::Next, response::Response};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use jsonwebtoken::{DecodingKey, Validation, decode};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::Duration;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: usize,
    pub iat: usize,
}

pub fn jwt_secret() -> String {
    std::env::var("JWT_SECRET").expect("JWT_SECRET must be set")
}

pub async fn logout(jar: CookieJar) -> (CookieJar, Json<serde_json::Value>) {
    let cookie = Cookie::build(("token", ""))
        .path("/")
        .http_only(true)
        .max_age(Duration::seconds(0))
        .build();

    (jar.add(cookie), Json(json!({ "success": true })))
}

pub async fn auth_middleware(
    cookie_jar: CookieJar,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let token = cookie_jar
        .get("token")
        .map(|c| c.value().to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let secret = jwt_secret();

    decode::<Claims>(
        &token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    Ok(next.run(request).await)
}

pub async fn me() -> Json<serde_json::Value> {
    Json(json!({ "authenticated": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::{Router, middleware, routing::get};
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serial_test::serial;
    use time::{Duration, OffsetDateTime};
    use tower::ServiceExt;

    #[tokio::test]
    #[serial]
    async fn test_logout_clears_cookie() {
        let jar = CookieJar::new();
        let (jar, _) = logout(jar).await;
        let cookie = jar.get("token").unwrap();
        assert_eq!(cookie.value(), "");
        assert_eq!(cookie.max_age(), Some(Duration::seconds(0)));
    }

    #[tokio::test]
    #[serial]
    async fn test_me_returns_authenticated() {
        let response = me().await;
        assert_eq!(response.0.get("authenticated"), Some(&json!(true)));
    }

    #[tokio::test]
    #[serial]
    async fn test_auth_middleware_no_cookie() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(auth_middleware));

        let res = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    #[serial]
    async fn test_auth_middleware_invalid_token() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(auth_middleware));

        let res = temp_env::async_with_vars([("JWT_SECRET", Some("test_secret"))], async {
            app.oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .header("Cookie", "token=invalid.jwt.here")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
        })
        .await;

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    #[serial]
    async fn test_auth_middleware_valid_token() {
        let now = OffsetDateTime::now_utc();
        let claims = Claims {
            sub: "admin".into(),
            exp: (now + Duration::days(1)).unix_timestamp() as usize,
            iat: now.unix_timestamp() as usize,
        };

        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(middleware::from_fn(auth_middleware));

        let res = temp_env::async_with_vars([("JWT_SECRET", Some("test_secret"))], async {
            let token = encode(
                &Header::default(),
                &claims,
                &EncodingKey::from_secret(jwt_secret().as_bytes()),
            )
            .unwrap();
            app.oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .header("Cookie", format!("token={}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
        })
        .await;

        assert_eq!(res.status(), StatusCode::OK);
    }
}
