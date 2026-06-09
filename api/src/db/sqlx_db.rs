use super::{DbError, DbParam, DbResult, DbValue, Row};
use sqlx::{Column, Row as _, SqlitePool, sqlite::SqlitePoolOptions};

#[derive(Clone)]
pub struct SqlxDatabase {
    pool: SqlitePool,
}

impl SqlxDatabase {
    pub async fn new(database_url: &str) -> Self {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(database_url)
            .await
            .expect("Failed to create sqlx database pool");
        Self { pool }
    }

    pub async fn execute(&self, sql: &str, params: &[DbParam]) -> DbResult<u64> {
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
        for p in params {
            q = match p {
                DbParam::Text(s) => q.bind(s.clone()),
                DbParam::Int(i) => q.bind(*i),
                DbParam::Null => q.bind(None::<String>),
            };
        }
        let result = q
            .execute(&self.pool)
            .await
            .map_err(|e| DbError::Database(e.to_string()))?;
        Ok(result.rows_affected())
    }

    async fn query_rows(&self, sql: &str, params: &[DbParam]) -> DbResult<Vec<Row>> {
        let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.to_owned()));
        for p in params {
            q = match p {
                DbParam::Text(s) => q.bind(s.clone()),
                DbParam::Int(i) => q.bind(*i),
                DbParam::Null => q.bind(None::<String>),
            };
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| DbError::Database(e.to_string()))?;

        if rows.is_empty() {
            return Ok(vec![]);
        }

        let columns: Vec<String> = rows[0]
            .columns()
            .iter()
            .map(|c| c.name().to_string())
            .collect();

        let mut result = Vec::new();
        for row in &rows {
            let mut values = Vec::new();
            for col in &columns {
                let val: DbValue = {
                    let raw: Result<String, _> = row.try_get(col.as_str());
                    let int: Result<i64, _> = row.try_get(col.as_str());
                    match (raw, int) {
                        (Ok(s), _) => DbValue::Text(s),
                        (_, Ok(i)) => DbValue::Int(i),
                        _ => {
                            let opt: Result<Option<String>, _> = row.try_get(col.as_str());
                            match opt {
                                Ok(Some(s)) => DbValue::Text(s),
                                Ok(None) => DbValue::Null,
                                _ => DbValue::Null,
                            }
                        }
                    }
                };
                values.push(val);
            }
            result.push(Row::new(columns.clone(), values));
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
