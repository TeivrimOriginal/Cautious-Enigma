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

/// Разбирает строку подключения в параметры драйвера.
///
/// Отдельная функция, потому что `DATABASE_URL` приходит из окружения, и
/// опечатка в нём — это ошибка конфигурации, а не паника. Разбор вынесен,
/// чтобы правило «плохой URL не роняет процесс» проверялось без базы.
pub fn connect_options(database_url: &str) -> AppResult<PgConnectOptions> {
    PgConnectOptions::from_str(database_url)
        .map(|options| options.application_name("linguarust"))
        .map_err(|err| {
            AppError::Config(crate::config::ConfigError::InvalidDatabaseUrl(
                err.to_string(),
            ))
        })
}

/// Создаёт пул соединений и применяет миграции.
pub async fn init(cfg: &Config) -> AppResult<PgPool> {
    let options = connect_options(&cfg.database_url)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::ConnectOptions as _;

    #[test]
    fn a_valid_url_is_parsed() {
        let options = connect_options("postgres://user:pass@127.0.0.1:5432/linguarust")
            .expect("корректная строка подключения разбирается");
        let rendered = options.to_url_lossy();
        assert!(
            rendered.as_str().contains("linguarust"),
            "хост и база потеряны: {rendered}"
        );
    }

    #[test]
    fn the_application_name_is_set() {
        // Без него в `pg_stat_activity` все соединения выглядят одинаково,
        // и в журнале PostgreSQL не видно, кто занял пул.
        let options = connect_options("postgres://user:pass@127.0.0.1:5432/linguarust")
            .expect("корректная строка подключения разбирается");
        let name = options
            .get_application_name()
            .expect("application_name задан конструктором выше");
        assert_eq!(name, "linguarust");
    }

    #[test]
    fn a_broken_url_is_a_config_error_not_a_panic() {
        // `DATABASE_URL` приходит из окружения: опечатка обязана дать
        // понятную ошибку, а не уронить процесс на старте.
        //
        // `postgres://` и `mysql://` в список не входят: драйвер sqlx
        // разбирает любую URL-строку и подставляет значения по умолчанию —
        // то есть проверять надо то, что действительно не разбирается.
        // Отдельным тестом ниже зафиксировано это поведение.
        for broken in [
            "",
            "not-a-url",
            "postgres://user:pass@host:port/db",
            "postgres://user:pass@127.0.0.1:5432/db?port=abc",
        ] {
            let result = connect_options(broken);
            assert!(result.is_err(), "битая строка не принята: {broken:?}");
        }
    }

    #[test]
    fn a_url_of_another_scheme_is_still_accepted_by_the_driver() {
        // Фиксирует решение, а не желание: `PgConnectOptions` не смотрит на
        // схему, поэтому `mysql://` молча станет обычным подключением к
        // PostgreSQL. Если драйвер поменяет поведение, тест покажет, что
        // приложение на это не рассчитывало.
        assert!(connect_options("postgres://").is_ok());
        assert!(connect_options("mysql://user:pass@127.0.0.1/db").is_ok());
    }

    #[test]
    fn a_broken_url_never_names_the_password() {
        // Сообщение об ошибке попадает в логи и, возможно, в ответ:
        // пароль из строки подключения там быть не должен.
        let secret = "s3cr3t-pass";
        let broken = format!("postgres://user:{secret}@host:not-a-port/db");
        let error = connect_options(&broken).expect_err("битый порт");
        let message = error.public_message();
        assert!(!message.contains(secret), "пароль утёк: {message}");
    }

    #[test]
    fn migrations_are_embedded_at_build_time() {
        // Пустой набор миграций означал бы, что `include` не сработал и
        // база осталась бы без таблиц — и это выяснилось бы только в рантайме.
        assert!(!MIGRATOR.migrations.is_empty(), "миграции не встроены");
    }
}
