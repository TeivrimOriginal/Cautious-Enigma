//! LinguaRust — SSR-сайт для изучения английского языка.
//!
//! Стек: Axum (HTTP) + SQLx (PostgreSQL) + Askama (шаблоны) + минимум JS.
//! Один и тот же `Router` используется и локальным сервером, и serverless-функцией Vercel.

pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod models;
pub mod queries;
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
use crate::routes::{api, cards, grammar, home, reading};

/// Статика встраивается в бинарник: на serverless нет доступа к файловой системе.
pub const STYLE_CSS: &str = include_str!("../static/style.css");
pub const APP_JS: &str = include_str!("../static/app.js");

/// Состояние приложения, доступное всем обработчикам через `State`.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub cfg: Arc<Config>,
}

/// Собирает полный роутер приложения.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Страницы
        .route("/", get(home::index))
        .route("/welcome", get(home::welcome))
        .route("/profile", post(home::create_profile))
        .route("/profile/reset", post(home::reset_profile))
        .route("/cards", get(cards::index))
        .route("/cards", post(cards::create))
        .route("/cards/{id}/review", post(cards::review))
        .route("/cards/{id}/delete", post(cards::delete))
        .route("/reading", get(reading::index))
        .route("/reading/{slug}", get(reading::show))
        .route("/grammar", get(grammar::index))
        .route("/stats", get(stats_page::index))
        .route("/healthz", get(home::health))
        // JSON API
        .route("/api/translate", get(api::translate))
        .route("/api/cards", post(api::add_card))
        .route("/api/grammar/check", post(api::check_grammar))
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
