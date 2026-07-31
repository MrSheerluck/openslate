mod auth;
mod config;
mod db;
mod media;
mod notes;
mod preferences;
mod search;
mod users;

use axum::extract::FromRef;
use axum::http::Method;
use axum::routing::{get, post, put};
use axum::{Router, middleware};
use s3::{Auth, Client, Credentials, providers};
use tower_http::cors::AllowOrigin;

#[derive(Clone)]
struct AppState {
    db: sqlx::SqlitePool,
    client: Option<Client>,
    bucket: Option<String>,
}

impl FromRef<AppState> for sqlx::SqlitePool {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let config = config::Config::from_env();

    let pool = db::create_pool(&config.database_url).await;
    db::run_migrations(&pool).await;

    let (client, bucket) =
        if let (Some(account_id), Some(access_key), Some(secret_key), Some(bucket_name)) = (
            &config.r2_account_id,
            &config.r2_access_key,
            &config.r2_secret_key,
            &config.r2_bucket,
        ) {
            let preset = providers::cloudflare_r2(account_id, providers::R2Endpoint::Global)
                .expect("Failed to create R2 preset");

            let c = Client::builder(preset.endpoint())
                .unwrap()
                .region(preset.region())
                .addressing_style(preset.addressing_style())
                .auth(Auth::Static(
                    Credentials::new(access_key, secret_key).unwrap(),
                ))
                .build()
                .expect("Failed to build S3 client");

            (Some(c), Some(bucket_name))
        } else {
            eprintln!("Warning: R2 not configured — media uploads disabled");
            (None, None)
        };

    let state = AppState {
        db: pool,
        client,
        bucket: bucket.cloned(),
    };

    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin(AllowOrigin::exact(config.frontend_url.parse().unwrap()))
        .allow_credentials(true)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([axum::http::header::CONTENT_TYPE]);

    let public = Router::new()
        .route("/api/health", get(health_check))
        .route("/api/auth/status", get(users::status))
        .route("/api/auth/signup", post(users::signup))
        .route("/api/auth/signin", post(users::signin))
        .route("/api/auth/logout", post(auth::logout));

    let protected = Router::new()
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/password", put(users::change_password))
        .route(
            "/api/notes",
            get(notes::list_notes).post(notes::create_note),
        )
        .route("/api/notes/import", post(notes::import_notes))
        .route("/api/notes/export", get(notes::export_notes))
        .route("/api/notes/export-by-tag", get(notes::export_notes_by_tag))
        .route(
            "/api/notes/{slug}",
            get(notes::get_note)
                .put(notes::update_note)
                .delete(notes::delete_note),
        )
        .route("/api/search", get(search::search_notes))
        .route(
            "/api/media",
            get(media::list_media).post(media::upload_media),
        )
        .route("/api/media/from-url", post(media::import_from_url))
        .route(
            "/api/media/{id}",
            get(media::get_media)
                .delete(media::delete_media)
                .put(media::update_media),
        )
        .route("/api/media/{id}/file", get(media::serve_media_file))
        .route(
            "/api/preferences",
            get(preferences::get_preferences).put(preferences::update_preferences),
        )
        .route_layer(middleware::from_fn(auth::auth_middleware));

    let app = Router::new()
        .merge(public)
        .merge(protected)
        .layer(cors)
        .with_state(state);

    let addr = format!("{}:{}", config.host, config.port);
    println!("Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health_check(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Result<axum::Json<serde_json::Value>, axum::http::StatusCode> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|_| axum::http::StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(axum::Json(serde_json::json!({ "status": "ok" })))
}
