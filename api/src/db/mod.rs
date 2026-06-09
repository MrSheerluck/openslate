#[cfg(feature = "backend-turso")]
mod libsql_db;
mod sqlx_db;

#[cfg(feature = "backend-turso")]
pub use libsql_db::LibsqlDatabase;
pub use sqlx_db::SqlxDatabase;

pub type DbResult<T> = Result<T, DbError>;

#[derive(Debug)]
pub enum DbError {
    Database(String),
    NotFound,
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Database(msg) => write!(f, "database error: {}", msg),
            DbError::NotFound => write!(f, "row not found"),
        }
    }
}

impl std::error::Error for DbError {}

#[derive(Clone)]
pub enum DbParam {
    Text(String),
    Int(i64),
    Null,
}

impl From<&str> for DbParam {
    fn from(s: &str) -> Self {
        DbParam::Text(s.to_string())
    }
}

impl From<String> for DbParam {
    fn from(s: String) -> Self {
        DbParam::Text(s)
    }
}

impl From<i64> for DbParam {
    fn from(i: i64) -> Self {
        DbParam::Int(i)
    }
}

impl From<Option<String>> for DbParam {
    fn from(opt: Option<String>) -> Self {
        match opt {
            Some(s) => DbParam::Text(s),
            None => DbParam::Null,
        }
    }
}

#[derive(Clone)]
pub enum DbValue {
    Text(String),
    Int(i64),
    Null,
}

impl DbValue {
    pub fn as_string_ref(&self) -> String {
        match self {
            DbValue::Text(s) => s.clone(),
            DbValue::Int(i) => i.to_string(),
            DbValue::Null => String::new(),
        }
    }

    pub fn as_i64(&self) -> DbResult<i64> {
        match self {
            DbValue::Int(i) => Ok(*i),
            DbValue::Text(s) => s
                .parse()
                .map_err(|_| DbError::Database(format!("cannot parse '{}' as i64", s))),
            DbValue::Null => Err(DbError::Database("unexpected null".into())),
        }
    }

    pub fn as_opt_string_ref(&self) -> Option<String> {
        match self {
            DbValue::Text(s) => Some(s.clone()),
            DbValue::Int(i) => Some(i.to_string()),
            DbValue::Null => None,
        }
    }
}

#[derive(Clone)]
pub struct Row {
    columns: Vec<String>,
    values: Vec<DbValue>,
}

impl Row {
    pub(crate) fn new(columns: Vec<String>, values: Vec<DbValue>) -> Self {
        Self { columns, values }
    }

    fn val(&self, col: &str) -> DbResult<&DbValue> {
        self.columns
            .iter()
            .position(|c| c == col)
            .and_then(|i| self.values.get(i))
            .ok_or_else(|| DbError::Database(format!("column '{}' not found", col)))
    }

    pub fn str(&self, col: &str) -> DbResult<String> {
        Ok(self.val(col)?.as_string_ref())
    }

    pub fn i64(&self, col: &str) -> DbResult<i64> {
        self.val(col)?.as_i64()
    }

    pub fn opt_str(&self, col: &str) -> DbResult<Option<String>> {
        Ok(self.val(col)?.as_opt_string_ref())
    }

    pub fn first_str(&self) -> DbResult<String> {
        self.values
            .first()
            .ok_or_else(|| DbError::Database("empty row".into()))
            .map(|v| v.as_string_ref())
    }

    pub fn first_i64(&self) -> DbResult<i64> {
        self.values
            .first()
            .ok_or_else(|| DbError::Database("empty row".into()))
            .and_then(|v| v.as_i64())
    }
}

pub trait FromRow: Sized {
    fn from_row(row: &Row) -> DbResult<Self>;
}

#[derive(Clone)]
pub enum Database {
    Sqlite(SqlxDatabase),
    #[cfg(feature = "backend-turso")]
    Turso(LibsqlDatabase),
}

impl Database {
    pub async fn execute(&self, sql: &str, params: &[DbParam]) -> DbResult<u64> {
        match self {
            Database::Sqlite(db) => db.execute(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.execute(sql, params).await,
        }
    }

    #[allow(dead_code)]
    pub async fn scalar_str(&self, sql: &str, params: &[DbParam]) -> DbResult<String> {
        match self {
            Database::Sqlite(db) => db.scalar_str(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.scalar_str(sql, params).await,
        }
    }

    pub async fn scalar_str_opt(&self, sql: &str, params: &[DbParam]) -> DbResult<Option<String>> {
        match self {
            Database::Sqlite(db) => db.scalar_str_opt(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.scalar_str_opt(sql, params).await,
        }
    }

    pub async fn scalar_str_all(&self, sql: &str, params: &[DbParam]) -> DbResult<Vec<String>> {
        match self {
            Database::Sqlite(db) => db.scalar_str_all(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.scalar_str_all(sql, params).await,
        }
    }

    pub async fn scalar_i64(&self, sql: &str, params: &[DbParam]) -> DbResult<i64> {
        match self {
            Database::Sqlite(db) => db.scalar_i64(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.scalar_i64(sql, params).await,
        }
    }

    #[allow(dead_code)]
    pub async fn row_one<T: FromRow>(&self, sql: &str, params: &[DbParam]) -> DbResult<T> {
        match self {
            Database::Sqlite(db) => db.row_one::<T>(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.row_one::<T>(sql, params).await,
        }
    }

    pub async fn row_opt<T: FromRow>(&self, sql: &str, params: &[DbParam]) -> DbResult<Option<T>> {
        match self {
            Database::Sqlite(db) => db.row_opt::<T>(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.row_opt::<T>(sql, params).await,
        }
    }

    pub async fn row_all<T: FromRow>(&self, sql: &str, params: &[DbParam]) -> DbResult<Vec<T>> {
        match self {
            Database::Sqlite(db) => db.row_all::<T>(sql, params).await,
            #[cfg(feature = "backend-turso")]
            Database::Turso(db) => db.row_all::<T>(sql, params).await,
        }
    }
}

pub async fn create_database(database_url: &str) -> Database {
    if database_url.starts_with("libsql://") {
        #[cfg(feature = "backend-turso")]
        {
            Database::Turso(LibsqlDatabase::new(database_url).await)
        }
        #[cfg(not(feature = "backend-turso"))]
        {
            panic!("Turso backend not enabled. Rebuild with --features backend-turso");
        }
    } else {
        Database::Sqlite(SqlxDatabase::new(database_url).await)
    }
}

pub async fn run_migrations(db: &Database) -> DbResult<()> {
    db.execute(
        "CREATE TABLE IF NOT EXISTS _migrations (name TEXT PRIMARY KEY, applied_at TEXT DEFAULT (datetime('now')))",
        &[],
    )
    .await?;

    for (name, sql) in MIGRATIONS {
        let count = db
            .scalar_i64(
                "SELECT COUNT(*) FROM _migrations WHERE name = ?",
                &[DbParam::Text(name.to_string())],
            )
            .await
            .unwrap_or(0);

        if count == 0 {
            db.execute(sql, &[]).await?;
            db.execute(
                "INSERT INTO _migrations (name) VALUES (?)",
                &[DbParam::Text(name.to_string())],
            )
            .await?;
        }
    }
    Ok(())
}

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "001_initial",
        include_str!("../../migrations/001_initial.sql"),
    ),
    (
        "002_preferences",
        include_str!("../../migrations/002_preferences.sql"),
    ),
    ("003_fts", include_str!("../../migrations/003_fts.sql")),
    ("004_media", include_str!("../../migrations/004_media.sql")),
    ("005_users", include_str!("../../migrations/005_users.sql")),
];
