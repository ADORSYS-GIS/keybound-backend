use backend_core::{Error, Result};
use sqlx::postgres::PgPoolOptions;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/");

#[derive(Clone, Copy, Debug)]
enum DbKind {
    Postgres,
}

/// Builder/factory for constructing database pools with migrations.
#[derive(Clone, Debug)]
pub struct DbFactory {
    url: String,
    max_connections: u32,
    kind: DbKind,
}

impl DbFactory {
    pub fn postgres(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            max_connections: 5,
            kind: DbKind::Postgres,
        }
    }

    pub fn max_connections(mut self, max: u32) -> Self {
        self.max_connections = max;
        self
    }

    pub async fn build_postgres(self) -> Result<sqlx::PgPool> {
        if !matches!(self.kind, DbKind::Postgres) {
            return Err(Error::Database(
                "DbFactory::build_postgres called on non-postgres factory".to_string(),
            ));
        }
        
        let pool = PgPoolOptions::new()
            .max_connections(self.max_connections)
            .connect(&self.url)
            .await
            .map_err(|e| Error::Database(e.to_string()))?;

        MIGRATOR.run(&pool).await.map_err(|e| Error::Database(e.to_string()))?;
        Ok(pool)
    }
}

/// Connect using PgPool and run the provided migrator.
pub async fn connect_postgres_and_migrate(
    database_url: &str,
) -> Result<sqlx::PgPool> {
    DbFactory::postgres(database_url).build_postgres().await
}
