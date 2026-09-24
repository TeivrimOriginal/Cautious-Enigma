//! Serverless-функция для Vercel.
//!
//! Весь сайт — одна функция: `vercel.json` перенаправляет любой путь на
//! `/api/axum`, поэтому маршрутизацию целиком берёт на себя Axum.
//! `VercelLayer` адаптирует `Router` к HTTP-событию Vercel.

use linguarust::{build_router, init_state, init_tracing};
use tower::ServiceBuilder;
use vercel_runtime::axum::VercelLayer;

/// Ошибка загрузки: конфигурация/БД и ошибки самого рантайма Vercel
/// не приводятся к общему типу — это исключает лишние `From`-реализации.
enum BootError {
    Init(Box<dyn std::error::Error + Send + Sync>),
    Runtime(vercel_runtime::Error),
}

impl std::fmt::Debug for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::Init(err) => write!(f, "не удалось инициализировать приложение: {err}"),
            BootError::Runtime(err) => write!(f, "ошибка serverless-рантайма: {err}"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), BootError> {
    dotenvy::dotenv().ok();
    init_tracing();

    // Холодный старт: подключение к БД, миграции и сиды выполняются один раз
    // на инстанс, дальше работают тёплые вызовы.
    let state = init_state().await.map_err(BootError::Init)?;
    let app = build_router(state);

    let service = ServiceBuilder::new().layer(VercelLayer::new()).service(app);

    vercel_runtime::run(service)
        .await
        .map_err(BootError::Runtime)
}
