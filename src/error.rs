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

    /// Слишком много попыток. `retry_after` — сколько секунд ждать.
    #[error("слишком много попыток, повторите через {0} с")]
    TooManyRequests(u64),

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
            AppError::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
        }
    }

    /// Краткое описание для пользователя: внутренние детали БД не раскрываем.
    pub fn public_message(&self) -> String {
        match self {
            AppError::NotFound => "Страница не найдена".to_string(),
            AppError::Unauthorized => "Войдите или зарегистрируйтесь, чтобы продолжить".to_string(),
            AppError::BadRequest(msg) | AppError::Conflict(msg) => msg.clone(),
            AppError::TooManyRequests(seconds) => {
                format!("Слишком много попыток. Повторите через {seconds} с")
            }
            // Текст ошибки БД клиенту не показываем: он раскрывает
            // структуру соединения, версии драйвера и имена хостов.
            AppError::Database(_) => "База данных недоступна. Попробуйте позже".to_string(),
            // Текст ошибки драйвера может содержать логин и пароль из
            // `DATABASE_URL`, поэтому наружу уходит только имя переменной:
            // детали остаются в `tracing` через `log()`.
            AppError::Config(crate::config::ConfigError::InvalidDatabaseUrl(_)) => {
                "Строка подключения к базе задана неверно. Проверьте DATABASE_URL".to_string()
            }
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
        let response = (status, Html(body.render().unwrap_or_else(|_| fallback(status))))
            .into_response();
        attach_retry_after(response, &self)
    }
}

/// Добавляет `Retry-After` при 429.
///
/// Заголовок обязателен: без него клиент не знает, когда можно повторить,
/// и просто долбит форму, удерживая лимит в трюме.
fn attach_retry_after(mut response: Response, error: &AppError) -> Response {
    if let AppError::TooManyRequests(seconds) = error {
        let value = seconds.to_string();
        if let Ok(header) = axum::http::HeaderValue::from_str(&value) {
            response
                .headers_mut()
                .insert(axum::http::header::RETRY_AFTER, header);
        }
    }
    response
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
            AppError::TooManyRequests(_) => "too_many_requests",
            _ => "internal_error",
        };
        let body = ApiErrorBody {
            error: self.0.public_message(),
            code,
        };
        let response = (self.0.status(), Json(body)).into_response();
        attach_retry_after(response, &self.0)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_match_the_error_kind() {
        assert_eq!(AppError::NotFound.status(), StatusCode::NOT_FOUND);
        assert_eq!(AppError::Unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            AppError::BadRequest("x".into()).status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            AppError::Conflict("x".into()).status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            AppError::TooManyRequests(60).status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn database_failures_do_not_leak_internals() {
        // Текст ошибки драйвера раскрывает хост, порт и версию, поэтому
        // наружу уходит только короткая фраза.
        for inner in [
            sqlx::Error::PoolTimedOut,
            sqlx::Error::RowNotFound,
            sqlx::Error::ColumnNotFound("secret_column".into()),
        ] {
            let error = AppError::Database(inner);
            assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let message = error.public_message();
            assert!(message.contains("База данных недоступна"), "{message}");
            assert!(
                !message.contains("pool timed out")
                    && !message.contains("secret_column")
                    && !message.contains("row not found"),
                "внутренние детали не показываем: {message}"
            );
        }
    }

    #[test]
    fn migration_failures_do_not_leak_the_migration_text() {
        let error = AppError::Migration(sqlx::migrate::MigrateError::VersionMismatch(7));
        let message = error.public_message();
        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(!message.contains("VersionMismatch"), "{message}");
    }

    #[test]
    fn internal_errors_are_generic() {
        let error = AppError::Internal("секретный путь /etc/passwd".into());
        assert!(
            !error.public_message().contains("/etc/passwd"),
            "сообщение: {}",
            error.public_message()
        );
    }

    #[test]
    fn client_errors_keep_their_message() {
        let error = AppError::BadRequest("Слово длиннее 100 символов".into());
        assert_eq!(error.public_message(), "Слово длиннее 100 символов");
    }

    #[test]
    fn rate_limit_message_states_the_wait() {
        let error = AppError::TooManyRequests(120);
        assert!(error.public_message().contains("120"));
    }

    #[test]
    fn html_error_page_carries_status_and_message() {
        let response = AppError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn rate_limited_response_sends_retry_after() {
        let response = AppError::TooManyRequests(90).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        let header = response
            .headers()
            .get("retry-after")
            .expect("без Retry-After клиент не знает, когда повторить");
        assert_eq!(header, "90");
    }

    #[test]
    fn other_errors_have_no_retry_after_header() {
        for make in [
            || AppError::NotFound,
            || AppError::Unauthorized,
            || AppError::BadRequest("x".into()),
        ] {
            let response = make().into_response();
            assert!(
                response.headers().get("retry-after").is_none(),
                "лишний заголовок у {response:?}"
            );
        }
    }

    #[test]
    fn api_error_body_carries_a_machine_code() {
        let response = ApiError::from(AppError::NotFound).into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn api_rate_limit_is_marked_with_its_own_code() {
        let response = ApiError::from(AppError::TooManyRequests(30)).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            response.headers().get("retry-after").map(|v| v.to_str().ok()),
            Some(Some("30"))
        );
    }

    #[test]
    fn a_broken_database_url_names_the_variable_not_the_secret() {
        // Ошибка драйвера про строку подключения может содержать логин и
        // пароль: наружу уходит только имя переменной.
        let error = AppError::Config(crate::config::ConfigError::InvalidDatabaseUrl(
            "invalid port: s3cr3t".into(),
        ));
        assert_eq!(error.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let message = error.public_message();
        assert!(message.contains("DATABASE_URL"), "сообщение: {message}");
        assert!(!message.contains("s3cr3t"), "пароль утёк: {message}");
        assert!(!message.contains("invalid port"), "детали драйвера: {message}");
        // Детали остаются в тексте ошибки — они попадают в журнал, а не в HTTP.
        let internal = error.to_string();
        assert!(internal.contains("DATABASE_URL"));
        assert!(internal.contains("s3cr3t"), "в журнале детали нужны: {internal}");
    }

    #[test]
    fn panic_handler_hides_the_payload() {
        let response = handle_panic(Box::new("секретный хеш пароля"));
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
