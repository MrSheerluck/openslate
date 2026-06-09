use std::sync::Arc;

use super::{DbError, DbParam, DbResult, DbValue, Row};
use libsql::Database as LibsqlInner;

#[derive(Clone)]
pub struct LibsqlDatabase {
    db: Arc<LibsqlInner>,
}

impl LibsqlDatabase {
    pub async fn new(database_url: &str) -> Self {
        let (url, token) = parse_turso_url(database_url);
        let db = libsql::Builder::new_remote(url, token)
            .build()
            .await
            .expect("Failed to connect to Turso");
        Self { db: Arc::new(db) }
    }

    fn conn(&self) -> DbResult<libsql::Connection> {
        self.db
            .connect()
            .map_err(|e| DbError::Database(e.to_string()))
    }

    pub async fn execute(&self, sql: &str, params: &[DbParam]) -> DbResult<u64> {
        let conn = self.conn()?;
        let p = to_libsql_params(params);
        conn.execute(sql, p)
            .await
            .map_err(|e| DbError::Database(e.to_string()))
    }

    async fn query_rows(&self, sql: &str, params: &[DbParam]) -> DbResult<Vec<Row>> {
        let conn = self.conn()?;
        let p = to_libsql_params(params);
        let mut rows = conn
            .query(sql, p)
            .await
            .map_err(|e| DbError::Database(e.to_string()))?;

        let col_count = rows.column_count() as usize;
        let col_names: Vec<String> = (0..col_count)
            .filter_map(|i| rows.column_name(i as i32).map(|s| s.to_string()))
            .collect();

        let mut result = Vec::new();
        while let Ok(Some(row)) = rows.next().await {
            let mut values = Vec::new();
            for i in 0..col_count {
                let val = row
                    .get_value(i as i32)
                    .map(libsql_val_to_db_val)
                    .unwrap_or(DbValue::Null);
                values.push(val);
            }
            result.push(Row::new(col_names.clone(), values));
        }
        Ok(result)
    }

    #[allow(dead_code)]
    pub async fn scalar_str(&self, sql: &str, params: &[DbParam]) -> DbResult<String> {
        let rows = self.query_rows(sql, params).await?;
        rows.first()
            .ok_or(DbError::NotFound)
            .and_then(|r| r.first_str())
    }

    pub async fn scalar_str_opt(&self, sql: &str, params: &[DbParam]) -> DbResult<Option<String>> {
        let rows = self.query_rows(sql, params).await?;
        Ok(rows.first().map(|r| r.first_str()).transpose()?)
    }

    pub async fn scalar_str_all(&self, sql: &str, params: &[DbParam]) -> DbResult<Vec<String>> {
        let rows = self.query_rows(sql, params).await?;
        rows.iter().map(|r| r.first_str()).collect()
    }

    pub async fn scalar_i64(&self, sql: &str, params: &[DbParam]) -> DbResult<i64> {
        let rows = self.query_rows(sql, params).await?;
        rows.first()
            .ok_or(DbError::NotFound)
            .and_then(|r| r.first_i64())
    }

    #[allow(dead_code)]
    pub async fn row_one<T: super::FromRow>(&self, sql: &str, params: &[DbParam]) -> DbResult<T> {
        let rows = self.query_rows(sql, params).await?;
        let row = rows.into_iter().next().ok_or(DbError::NotFound)?;
        T::from_row(&row)
    }

    pub async fn row_opt<T: super::FromRow>(
        &self,
        sql: &str,
        params: &[DbParam],
    ) -> DbResult<Option<T>> {
        let rows = self.query_rows(sql, params).await?;
        match rows.into_iter().next() {
            Some(row) => Ok(Some(T::from_row(&row)?)),
            None => Ok(None),
        }
    }

    pub async fn row_all<T: super::FromRow>(
        &self,
        sql: &str,
        params: &[DbParam],
    ) -> DbResult<Vec<T>> {
        let rows = self.query_rows(sql, params).await?;
        rows.iter().map(T::from_row).collect()
    }
}

fn to_libsql_params(params: &[DbParam]) -> Vec<libsql::Value> {
    params
        .iter()
        .map(|p| match p {
            DbParam::Text(s) => libsql::Value::from(s.clone()),
            DbParam::Int(i) => libsql::Value::from(*i),
            DbParam::Null => libsql::Value::Null,
        })
        .collect()
}

fn libsql_val_to_db_val(v: libsql::Value) -> DbValue {
    match v {
        libsql::Value::Text(s) => DbValue::Text(s),
        libsql::Value::Integer(i) => DbValue::Int(i),
        libsql::Value::Real(f) => DbValue::Text(f.to_string()),
        libsql::Value::Blob(_) => DbValue::Text("[blob]".into()),
        libsql::Value::Null => DbValue::Null,
    }
}

fn parse_turso_url(url: &str) -> (String, String) {
    let url = url.strip_prefix("libsql://").unwrap_or(url);

    if let Some((host, query)) = url.split_once('?') {
        let base = format!("libsql://{}", host);
        let token = query
            .split('&')
            .find_map(|pair| {
                let (k, v) = pair.split_once('=')?;
                if k == "authToken" || k == "auth_token" {
                    Some(v.to_string())
                } else {
                    None
                }
            })
            .or_else(|| std::env::var("TURSO_AUTH_TOKEN").ok())
            .unwrap_or_else(|| {
                panic!("Turso auth token not found. Set TURSO_AUTH_TOKEN or include authToken in DATABASE_URL")
            });
        (base, token)
    } else {
        let token = std::env::var("TURSO_AUTH_TOKEN")
            .unwrap_or_else(|_| panic!("TURSO_AUTH_TOKEN must be set"));
        (format!("libsql://{}", url), token)
    }
}
