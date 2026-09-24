//! Типизированные ошибки приложения и их представление в HTTP-ответе.

use askama::Template;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::config::ConfigError;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("ошибка базы данных: {0}")]
    Database(#[from] sqlx::Error),

    #[error("ошибка миграций: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("ошибка конфигурации: {0}")]
    Config(#[from] ConfigError),

    #[error("ошибка отрисовки шаблона: {0}")]
    Template(#[from] askama::Error),

    #[error("запрошенная страница не найдена")]
    NotFound,

    #[error("нет активного профиля")]
    Unauthorized,

    #[error("некорректные данные: {0}")]
    BadRequest(String),

    #[error("такая запись уже существует")]
    Conflict(String),

    #[error("внутренняя ошибка сервера: {0}")]
    Internal(String),
}

impl AppError {
    pub fn status(&self) -> StatusCode {
        match self {
            AppError::Database(_) | AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Migration(_) | AppError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Template(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
        }
    }

    /// Краткое описание для пользователя: внутренние детали БД не раскрываем.
    pub fn public_message(&self) -> String {
        match self {
            AppError::NotFound => "Страница не найдена".to_string(),
            AppError::Unauthorized => "Войдите или зарегистрируйтесь, чтобы продолжить".to_string(),
            AppError::BadRequest(msg) | AppError::Conflict(msg) => msg.clone(),
            AppError::Database(err) => format!("База данных недоступна: {err}"),
            AppError::Migration(err) => format!("Не удалось применить миграции: {err}"),
            _ => "Внутренняя ошибка сервера. Попробуйте позже".to_string(),
        }
    }

    fn log(&self) {
        match self {
            AppError::Database(err) => tracing::error!(error = %err, "database error"),
            AppError::Migration(err) => tracing::error!(error = %err, "migration error"),
            AppError::Internal(msg) => tracing::error!(error = %msg, "internal error"),
            AppError::Template(err) => tracing::error!(error = %err, "template error"),
            other => tracing::debug!(error = %other, "client error"),
        }
    }
}

/// HTML-страница ошибки для браузерных маршрутов.
#[derive(Template)]
#[template(path = "error.html")]
struct ErrorPage {
    status: u16,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        self.log();
        let status = self.status();
        let body = ErrorPage {
            status: status.as_u16(),
            message: self.public_message(),
        };
        (
            status,
            Html(body.render().unwrap_or_else(|_| fallback(status))),
        )
            .into_response()
    }
}

fn fallback(status: StatusCode) -> String {
    format!(
        "<!doctype html><html lang=\"ru\"><meta charset=\"utf-8\">\
         <title>Ошибка {}</title><h1>Ошибка {}</h1>\
         <p>Не удалось отобразить страницу.</p></html>",
        status.as_u16(),
        status.as_u16()
    )
}

/// JSON-ошибка для `/api/*` маршрутов.
pub struct ApiError(pub AppError);

impl From<AppError> for ApiError {
    fn from(err: AppError) -> Self {
        Self(err)
    }
}

#[derive(serde::Serialize)]
struct ApiErrorBody {
    error: String,
    code: &'static str,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        self.0.log();
        let code = match self.0 {
            AppError::NotFound => "not_found",
            AppError::Unauthorized => "unauthorized",
            AppError::BadRequest(_) => "bad_request",
            AppError::Conflict(_) => "conflict",
            _ => "internal_error",
        };
        let body = ApiErrorBody {
            error: self.0.public_message(),
            code,
        };
        (self.0.status(), Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Обработчик паник для `CatchPanicLayer`: превращает панику в 500,
/// не раскрывая детали реализации клиенту.
pub fn handle_panic(_panic: Box<dyn std::any::Any + Send + 'static>) -> Response {
    tracing::error!("обработчик запроса завершился паникой");
    AppError::Internal("panic".into()).into_response()
}

/// Заглушка для неизвестных путей: отдаёт ту же страницу ошибки,
/// что и обработчик `AppError::NotFound`.
pub async fn not_found() -> Response {
    AppError::NotFound.into_response()
}
