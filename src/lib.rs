//! LinguaRust — SSR-сайт для изучения английского языка.
//!
//! Стек: Axum (HTTP) + SQLx (PostgreSQL) + Askama (шаблоны) + минимум JS.
//! Один и тот же `Router` используется и локальным сервером, и serverless-функцией Vercel.

pub mod anki;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod queries;
pub mod queue;
pub mod ratelimit;
pub mod routes;
pub mod seed;
pub mod sm2;
pub mod stats;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use tower_cookies::CookieManagerLayer;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::error::handle_panic;
use crate::routes::stats as stats_page;
use crate::routes::{api, auth as auth_pages, cards, dictionary, grammar, home, reading, study};

/// Статика встраивается в бинарник: на serverless нет доступа к файловой системе.
pub const STYLE_CSS: &str = include_str!("../static/style.css");
pub const APP_JS: &str = include_str!("../static/app.js");

/// Состояние приложения, доступное всем обработчикам через `State`.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub cfg: Arc<Config>,
    /// Счётчик попыток входа и регистрации. Живёт в состоянии, а не в
    /// статике, чтобы тесты поднимали независимый счётчик на каждый запуск.
    pub limiter: Arc<ratelimit::RateLimiter>,
}

/// Собирает полный роутер приложения.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Страницы
        .route("/", get(home::index))
        .route("/welcome", get(home::welcome))
        .route(
            "/register",
            get(auth_pages::register_form).post(auth_pages::register),
        )
        .route(
            "/login",
            get(auth_pages::login_form).post(auth_pages::login),
        )
        .route("/logout", post(auth_pages::logout))
        .route("/guest", post(auth_pages::guest))
        .route("/account", get(auth_pages::account))
        // Старые маршруты оставлены для ссылок из старых страниц.
        .route("/profile", post(home::create_profile))
        .route("/profile/reset", post(home::reset_profile))
        .route("/cards", get(cards::index))
        .route("/cards", post(cards::create))
        .route("/cards/{id}/review", post(cards::review))
        .route("/cards/{id}/edit", post(cards::update))
        .route("/cards/{id}/delete", post(cards::delete))
        .route("/cards/export", get(cards::export))
        .route("/cards/import", post(cards::import))
        .route("/study", get(study::index))
        .route("/study/{id}/review", post(study::review))
        .route("/dictionary", get(dictionary::index))
        .route("/reading", get(reading::index))
        .route("/reading/{slug}", get(reading::show))
        .route("/grammar", get(grammar::index))
        .route("/stats", get(stats_page::index))
        .route("/healthz", get(home::health))
        .fallback(error::not_found)
        // JSON API
        .route("/api/translate", get(api::translate))
        .route("/api/cards", post(api::add_card))
        .route("/api/grammar/check", post(api::check_grammar))
        .route("/api/goal", post(api::set_goal))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(CatchPanicLayer::custom(handle_panic))
        .layer(TraceLayer::new_for_http())
        .layer(CookieManagerLayer::new())
        .with_state(state)
}

/// Инициализация состояния: конфигурация, пул БД, миграции и сиды.
pub async fn init_state() -> Result<AppState, Box<dyn std::error::Error + Send + Sync>> {
    let cfg = Config::from_env()?;
    let db = db::init(&cfg).await?;
    Ok(AppState {
        db,
        cfg: Arc::new(cfg),
        limiter: Arc::new(ratelimit::RateLimiter::default()),
    })
}

/// Настраивает логирование. Уровень задаётся переменной `RUST_LOG`,
/// по умолчанию `info`. Повторный вызов безопасен.
pub fn init_tracing() {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("linguarust=info,tower_http=info,warn"));

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Стили и скрипт приходят `include_str!` на этапе сборки, поэтому
    /// бинарник, у которого они не встроились, упал бы с `include` —
    /// а вот пустые или обрезанные файлы прошли бы незаметно.
    #[test]
    fn styles_and_script_are_embedded() {
        assert!(STYLE_CSS.len() > 1_000, "стили выглядят пустыми");
        assert!(APP_JS.len() > 1_000, "скрипт выглядит пустым");
        assert!(STYLE_CSS.contains("body"), "в стилях нет базовых правил");
        assert!(APP_JS.contains("DOMContentLoaded"), "скрипт ничего не инициализирует");
    }

    /// Файл не должен закрываться на середине: обрезанный CSS ломает
    /// вёрстку, и заметить это можно только глазами.
    #[test]
    fn styles_are_not_truncated() {
        assert!(
            STYLE_CSS.trim_end().ends_with('}'),
            "стили кончились на середине блока"
        );
    }

    /// Скрипт обёрнут в IIFE, чтобы не выносить функции в глобальную область.
    /// Обрезанный скрипт потерял бы закрывающую скобку и обрушил бы
    /// весь следующий код страницы.
    #[test]
    fn the_script_is_wrapped_and_balanced() {
        // Перед IIFE идёт заголовочный комментарий, поэтому ищем обёртку
        // в тексте, а не в начале файла.
        assert!(
            APP_JS.contains("(function () {"),
            "нет IIFE: начало = {:?}",
            &APP_JS[..APP_JS.len().min(120)]
        );
        assert!(
            APP_JS.trim_end().ends_with("})();"),
            "нет закрытия IIFE: конец = {:?}",
            &APP_JS[APP_JS.len().saturating_sub(40)..]
        );
        let opens = APP_JS.matches('{').count();
        let closes = APP_JS.matches('}').count();
        assert_eq!(opens, closes, "скобки в скрипте не сбалансированы");
    }

    /// Скрипт выводит текст в `innerHTML` только через `escapeHtml` —
    /// это единственная защита от XSS на клиенте, и потерять её молча
    /// можно, отредактировав шаблон всплывающего окна.
    #[test]
    fn the_script_escapes_before_using_inner_html() {
        let assignments = APP_JS
            .match_indices("innerHTML =")
            .map(|(index, _)| &APP_JS[index..(index + 220).min(APP_JS.len())])
            .collect::<Vec<_>>();
        assert!(!assignments.is_empty(), "скрипт никуда не пишет разметку");

        for snippet in assignments {
            // Либо статичная строка без данных, либо только через escapeHtml.
            let interpolates = snippet.contains("${");
            let escaped = snippet.contains("escapeHtml(");
            assert!(
                !interpolates || escaped,
                "неэкранированная подстановка в innerHTML: {snippet}"
            );
        }
    }

    /// Роутер собирается из переданного состояния, а не из глобальных
    /// переменных: иначе тесты поднимали бы чужой пул соединений.
    ///
    /// `connect_lazy` требует контекста Tokio, поэтому тест асинхронный.
    #[tokio::test]
    async fn the_router_is_built_from_the_given_state() {
        use sqlx::postgres::PgPoolOptions;

        let config = Config {
            database_url: "postgres://user:pass@127.0.0.1:1/linguarust".to_string(),
            cookie_key: tower_cookies::Key::generate(),
            bind_addr: ([127, 0, 0, 1], 0).into(),
            max_connections: 1,
            is_production: false,
            acquire_timeout: std::time::Duration::from_millis(50),
        };
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect_lazy(&config.database_url)
            .expect("ленивый пул создаётся без базы");

        let app = build_router(AppState {
            db: pool,
            cfg: Arc::new(config),
            limiter: Arc::new(ratelimit::RateLimiter::default()),
        });
        // `Router` не имеет публичного счётчика маршрутов, поэтому проверяется
        // сам факт сборки: без состояния роутер собрать нельзя.
        let _ = app;
    }
}
