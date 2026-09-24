//! Тесты HTTP-слоя: проверяем роутер целиком, без подключения к базе.
//!
//! Гостевые страницы и заглушка 404 не ходят в PostgreSQL, поэтому пул
//! создаётся лениво (`connect_lazy`) и такие тесты идут в CI без базы.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use linguarust::config::Config;
use linguarust::{AppState, build_router};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use tower_cookies::Key;

/// Роутер с пулом, который никогда не подключается к реальной базе.
fn test_app() -> axum::Router {
    let config = Config {
        database_url: "postgres://user:pass@127.0.0.1:1/linguarust".to_string(),
        cookie_key: Key::generate(),
        bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        max_connections: 1,
        is_production: false,
        acquire_timeout: std::time::Duration::from_millis(200),
    };

    // connect_lazy не устанавливает соединение — тесты офлайн.
    // Короткие таймауты: /healthz должен быстро вернуть 503, а не ждать.
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_millis(200))
        .connect_lazy(&config.database_url)
        .expect("не удалось создать ленивый пул");

    build_router(AppState {
        db: pool,
        cfg: Arc::new(config),
    })
}

async fn get(path: &str) -> (StatusCode, String) {
    let response = test_app()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .expect("роутер не ответил");

    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("не удалось прочитать тело ответа")
        .to_bytes();
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn welcome_page_is_available_for_guests() {
    let (status, body) = get("/welcome").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Как вас зовут?"), "нет формы имени");
    assert!(body.contains("Пароля нет"), "нет пояснения про cookie");
}

#[tokio::test]
async fn root_redirects_guest_to_welcome_screen() {
    let (status, body) = get("/").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Привет! Это LinguaRust"));
}

#[tokio::test]
async fn registration_page_is_available_for_guests() {
    let (status, body) = get("/register").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Создать аккаунт"));
    assert!(body.contains("name=\"username\""));
    assert!(body.contains("name=\"password2\""));
}

#[tokio::test]
async fn login_page_preserves_validation_error() {
    let (status, body) = get("/login?error=Invalid+login&username=student_1").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Invalid login"));
    assert!(body.contains("value=\"student_1\""));
}

#[tokio::test]
async fn account_requires_a_session() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/account")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("роутер не ответил");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_without_session_redirects_home() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("роутер не ответил");
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers().get("location").unwrap(), "/");
}

#[tokio::test]
async fn unknown_path_renders_error_page() {
    let (status, body) = get("/definitely-missing-page").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("404"), "ожидалась страница ошибки");
    assert!(body.contains("Страница не найдена"));
}

#[tokio::test]
async fn api_translate_validates_input() {
    // Пустое слово должно дать 400, а не падение.
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/api/translate?word=")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("роутер не ответил");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn api_adding_card_requires_profile() {
    let response = test_app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/cards")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"front":"test","back":"тест"}"#))
                .unwrap(),
        )
        .await
        .expect("роутер не ответил");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn health_reports_unavailable_database() {
    let (status, body) = get("/healthz").await;
    // Пул без соединения: отвечаем 503, но не падаем.
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(body.contains("db unavailable"), "тело: {body}");
}
