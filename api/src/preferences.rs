use axum::{Json, extract::State, http::StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::AppState;
use crate::db::{self, FromRow, Row};

#[derive(Deserialize)]
pub struct UpdatePreferences {
    pub theme: Option<String>,
}

struct PrefRow {
    key: String,
    value: String,
}

impl FromRow for PrefRow {
    fn from_row(row: &Row) -> db::DbResult<Self> {
        Ok(Self {
            key: row.str("key")?,
            value: row.str("value")?,
        })
    }
}

pub async fn get_preferences(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let rows = state
        .db
        .row_all::<PrefRow>("SELECT key, value FROM preferences", &[])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut map = serde_json::Map::new();
    for row in rows {
        map.insert(row.key, Value::String(row.value));
    }

    Ok(Json(Value::Object(map)))
}

pub async fn update_preferences(
    State(state): State<AppState>,
    Json(body): Json<UpdatePreferences>,
) -> Result<Json<Value>, StatusCode> {
    if let Some(theme) = &body.theme {
        state
            .db
            .execute(
                "INSERT OR REPLACE INTO preferences (key, value) VALUES ('theme', ?)",
                &[theme.as_str().into()],
            )
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(json!({ "success": true })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, Database};

    async fn setup_db() -> Database {
        let db = db::SqlxDatabase::new("sqlite::memory:").await;
        let db = Database::Sqlite(db);

        db.execute(
            "CREATE TABLE preferences (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            &[],
        )
        .await
        .unwrap();

        db
    }

    #[tokio::test]
    async fn test_set_and_get_theme() {
        let db = setup_db().await;

        let state = crate::AppState {
            db: db.clone(),
            client: None,
            bucket: None,
        };

        let _ = update_preferences(
            State(state.clone()),
            Json(UpdatePreferences {
                theme: Some("dark".into()),
            }),
        )
        .await
        .unwrap();

        let prefs = get_preferences(State(state)).await.unwrap();
        assert_eq!(prefs.get("theme").unwrap(), &json!("dark"));
    }

    #[tokio::test]
    async fn test_get_empty_preferences() {
        let db = setup_db().await;
        let state = crate::AppState {
            db,
            client: None,
            bucket: None,
        };
        let prefs = get_preferences(State(state)).await.unwrap();
        assert!(prefs.as_object().unwrap().is_empty());
    }
}
