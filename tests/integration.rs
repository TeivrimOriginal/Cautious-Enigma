//! Интеграционные тесты слоя данных: реальные запросы к PostgreSQL.
//!
//! Без переменной `DATABASE_URL` тесты пропускаются, чтобы `cargo test`
//! работал и без базы (например, в CI). Запуск с базой:
//!
//! ```text
//! $env:DATABASE_URL = "postgres://linguarust:linguarust@localhost:5432/linguarust"
//! cargo test --test integration -- --nocapture
//! ```
//!
//! Тесты работают на уникальных профилях и удаляют их в конце: общая
//! база может использоваться параллельно с приложением.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use linguarust::config::Config;
use linguarust::db;
use linguarust::error::AppResult;
use linguarust::models::GrammarRow;
use linguarust::queries;
use linguarust::routes::today;
use linguarust::{AppState, build_router};
use sqlx::PgPool;
use tower::ServiceExt;
use tower_cookies::Key;
use uuid::Uuid;

/// Подключается к базе из `DATABASE_URL` и применяет миграции.
/// Возвращает `None`, если переменная не задана.
async fn init_pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let config = Config {
        database_url: url,
        cookie_key: Key::generate(),
        bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        max_connections: 2,
        is_production: false,
        acquire_timeout: std::time::Duration::from_secs(10),
    };
    db::init(&config).await.ok()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("не удалось создать runtime")
}

fn test_config(database_url: String) -> Config {
    Config {
        database_url,
        cookie_key: Key::generate(),
        bind_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
        max_connections: 3,
        is_production: false,
        acquire_timeout: std::time::Duration::from_secs(10),
    }
}

fn form_request(uri: &str, body: String, cookie: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    builder
        .body(Body::from(body))
        .expect("не удалось собрать запрос")
}

fn session_cookie(response: &axum::response::Response) -> String {
    response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|value| {
            let pair = value.split(';').next()?;
            pair.starts_with("lr_session=").then(|| pair.to_string())
        })
        .expect("в ответе нет cookie сессии")
}

async fn create_profile(pool: &PgPool, name: &str) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO profiles (id, name) VALUES ($1, $2)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(id)
}

async fn cleanup(pool: &PgPool, profile_id: Uuid) {
    let _ = sqlx::query("DELETE FROM profiles WHERE id = $1")
        .bind(profile_id)
        .execute(pool)
        .await;
}

#[test]
fn profile_and_cards_lifecycle() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };

        let profile_id = create_profile(&pool, "integration-test").await.unwrap();

        // Пустой профиль: всё по нулям.
        let summary = queries::review_summary(&pool, profile_id).await.unwrap();
        assert_eq!(summary.total_cards, 0);
        assert_eq!(summary.due_today, 0);
        assert_eq!(summary.total_reviews, 0);

        // Добавление карточки и защита от дублей.
        assert!(
            queries::insert_card(&pool, profile_id, "deadline", "срок сдачи", None)
                .await
                .unwrap()
        );
        assert!(
            !queries::insert_card(&pool, profile_id, "deadline", "дедлайн", None)
                .await
                .unwrap()
        );
        assert_eq!(queries::count_cards(&pool, profile_id).await.unwrap(), 1);

        // Новая карточка попадает в очередь на сегодня.
        let due = queries::due_cards(&pool, profile_id, 10).await.unwrap();
        assert_eq!(due.len(), 1);
        let card_id = due[0].id;

        // Первое успешное повторение: интервал 1 день, серия 1.
        let (state, next_due) = queries::apply_review(&pool, profile_id, card_id, 4, today())
            .await
            .unwrap();
        assert_eq!(state.repetitions, 1);
        assert_eq!(state.interval_days, 1);
        assert!((state.ease - 2.5).abs() < 1e-9);
        assert!(next_due > today());

        // Ошибка: серия сбрасывается, карточка снова доступна завтра.
        let (state, _) = queries::apply_review(&pool, profile_id, card_id, 1, today())
            .await
            .unwrap();
        assert_eq!(state.repetitions, 0);
        assert_eq!(state.interval_days, 1);

        let summary = queries::review_summary(&pool, profile_id).await.unwrap();
        assert_eq!(summary.total_reviews, 2);
        assert_eq!(summary.successful_reviews, 1);
        assert_eq!(summary.learned_cards, 0, "после ошибки слово снова новое");

        // Удаление каскадом убирает карточку и журнал.
        queries::delete_card(&pool, profile_id, card_id)
            .await
            .unwrap();
        let logs: i64 = sqlx::query_scalar("SELECT count(*) FROM review_log WHERE profile_id = $1")
            .bind(profile_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(logs, 0);

        cleanup(&pool, profile_id).await;
    });
}

#[test]
fn seed_data_is_available() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };

        let texts = queries::list_texts(&pool).await.unwrap();
        assert!(texts.len() >= 3, "тексты должны быть засеяны");
        assert!(texts.iter().all(|text| text.word_count > 0));

        let (exercise, number, total) = queries::exercise_at(&pool, 0).await.unwrap();
        assert_eq!(number, 1);
        assert!(total >= 15);
        let GrammarRow {
            options,
            correct_index,
            ..
        } = exercise;
        assert!(options.0.len() >= 2);
        assert!((correct_index as usize) < options.0.len());
    });
}

#[test]
fn auth_registration_login_logout() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };
        let config = test_config(
            std::env::var("DATABASE_URL").expect("DATABASE_URL исчез после инициализации"),
        );
        let app = build_router(AppState {
            db: pool.clone(),
            cfg: Arc::new(config),
        });
        let username = format!("u{}", &Uuid::new_v4().to_string()[..8]);

        let response = app
            .clone()
            .oneshot(form_request(
                "/register",
                format!(
                    "username={username}&password=correct-horse-battery&password2=correct-horse-battery"
                ),
                None,
            ))
            .await
            .expect("роутер не ответил");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = session_cookie(&response);
        let profile_id: Uuid = sqlx::query_scalar("SELECT id FROM profiles WHERE username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
        let password_hash: String =
            sqlx::query_scalar("SELECT password_hash FROM profiles WHERE id = $1")
                .bind(profile_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(password_hash.starts_with("$argon2id$"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/account")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("роутер не ответил");
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(form_request("/logout", String::new(), Some(&cookie)))
            .await
            .expect("роутер не ответил");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let sessions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM sessions WHERE profile_id = $1",
        )
        .bind(profile_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sessions, 0);

        let response = app
            .clone()
            .oneshot(form_request(
                "/login",
                format!("username={username}&password=correct-horse-battery"),
                None,
            ))
            .await
            .expect("роутер не ответил");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert!(session_cookie(&response).starts_with("lr_session="));

        cleanup(&pool, profile_id).await;
    });
}

#[test]
fn guest_registration_keeps_existing_progress() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };
        let config = test_config(
            std::env::var("DATABASE_URL").expect("DATABASE_URL исчез после инициализации"),
        );
        let app = build_router(AppState {
            db: pool.clone(),
            cfg: Arc::new(config),
        });
        let guest_name = format!("guest-{}", &Uuid::new_v4().to_string()[..8]);
        let username = format!("u{}", &Uuid::new_v4().to_string()[..8]);

        let response = app
            .clone()
            .oneshot(form_request(
                "/guest",
                format!("name={guest_name}"),
                None,
            ))
            .await
            .expect("роутер не ответил");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = session_cookie(&response);
        let profile_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM profiles WHERE name = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&guest_name)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(queries::insert_card(&pool, profile_id, "guest-word", "гостевой слово", None)
            .await
            .unwrap());

        let response = app
            .clone()
            .oneshot(form_request(
                "/register",
                format!("username={username}&password=correct-horse-battery&password2=correct-horse-battery"),
                Some(&cookie),
            ))
            .await
            .expect("роутер не ответил");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);

        let same_profile: Uuid = sqlx::query_scalar("SELECT id FROM profiles WHERE username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(same_profile, profile_id, "registration must not create a new profile");
        assert_eq!(queries::count_cards(&pool, profile_id).await.unwrap(), 1);

        cleanup(&pool, profile_id).await;
    });
}
