//! Локальный запуск: обычный HTTP-сервер Axum на TCP.
//!
//!     cargo run
//!
//! Требуется `DATABASE_URL` (можно в `.env`) и, для стабильных сессий,
//! `COOKIE_KEY` — 128 hex-символов.

use linguarust::{build_router, init_state, init_tracing};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenvy::dotenv().ok();
    init_tracing();

    let state = init_state().await?;
    let bind_addr = state.cfg.bind_addr;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!(address = %bind_addr, "LinguaRust запущен");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

/// Завершает сервер по Ctrl+C (на Vercel этим не пользуются).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("получен Ctrl+C, останавливаюсь"),
        _ = terminate => tracing::info!("получен SIGTERM, останавливаюсь"),
    }
}
