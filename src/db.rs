//! Подключение к PostgreSQL, миграции и первичное наполнение данными.

use std::str::FromStr;

use sqlx::PgPool;
use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

use crate::config::Config;
use crate::error::{AppError, AppResult};
use crate::seed;

/// Миграции встраиваются в бинарник на этапе сборки —
/// ни sqlx-cli, ни файлы миграций на сервере не нужны.
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Создаёт пул соединений и применяет миграции.
pub async fn init(cfg: &Config) -> AppResult<PgPool> {
    let options = PgConnectOptions::from_str(&cfg.database_url)
        .map_err(|err| AppError::Internal(format!("DATABASE_URL: {err}")))?
        .application_name("linguarust");

    let pool = PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .acquire_timeout(cfg.acquire_timeout)
        .idle_timeout(std::time::Duration::from_secs(30))
        .connect_with(options)
        .await?;

    MIGRATOR.run(&pool).await?;
    seed::run(&pool).await?;

    Ok(pool)
}
