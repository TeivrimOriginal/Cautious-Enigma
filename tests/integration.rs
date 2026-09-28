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
use http_body_util::BodyExt;
use linguarust::config::Config;
use linguarust::db;
use linguarust::error::AppResult;
use linguarust::models::GrammarRow;
use linguarust::queries;
use linguarust::routes::today;
use linguarust::{AppState, build_router, ratelimit};
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
fn due_queue_orders_and_limits_cards() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };

        let profile_id = create_profile(&pool, "integration-queue").await.unwrap();
        let today = today();

        // Три карточки: две просрочены, одна на сегодня. Даты выставляем
        // напрямую, потому что через SM-2 их не задать.
        let overdue: Vec<(i64, i32)> = vec![(1, 0), (2, 4)];
        for (index, repetitions) in overdue {
            let inserted = queries::insert_card(
                &pool,
                profile_id,
                &format!("queue-word-{index}"),
                "перевод",
                None,
            )
            .await
            .unwrap();
            assert!(inserted);
            sqlx::query("UPDATE cards SET due_date = $1, repetitions = $2 WHERE profile_id = $3 AND front = $4")
                .bind(today - chrono::Days::new(2))
                .bind(repetitions)
                .bind(profile_id)
                .bind(format!("queue-word-{index}"))
                .execute(&pool)
                .await
                .unwrap();
        }

        let inserted = queries::insert_card(&pool, profile_id, "queue-word-3", "перевод", None)
            .await
            .unwrap();
        assert!(inserted);

        // Будущая карточка в выдачу не попадает.
        sqlx::query("UPDATE cards SET due_date = $1 WHERE profile_id = $2 AND front = $3")
            .bind(today + chrono::Days::new(5))
            .bind(profile_id)
            .bind("queue-word-3")
            .execute(&pool)
            .await
            .unwrap();

        // Одна и та же дата: первым идёт менее повторённый, зрелый — вторым.
        let due = queries::due_cards(&pool, profile_id, 10).await.unwrap();
        let ids: Vec<i64> = due.iter().map(|card| card.id).collect();
        assert_eq!(ids.len(), 2, "просроченные: {ids:?}");
        assert_eq!(due[0].repetitions, 0, "порядок выдачи: {ids:?}");
        assert_eq!(due[1].repetitions, 4, "порядок выдачи: {ids:?}");

        // Лимит режет выдачу до указанного числа.
        let limited = queries::due_cards(&pool, profile_id, 1).await.unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].repetitions, 0, "первым идёт менее повторённый");

        // Неположительный лимит — пустая очередь, а не вся колода.
        assert!(queries::due_cards(&pool, profile_id, 0).await.unwrap().is_empty());
        assert!(queries::due_cards(&pool, profile_id, -1).await.unwrap().is_empty());

        cleanup(&pool, profile_id).await;
    });
}

#[test]
fn card_belongs_to_exactly_one_profile() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };

        let owner = create_profile(&pool, "integration-owner").await.unwrap();
        let stranger = create_profile(&pool, "integration-stranger").await.unwrap();

        queries::insert_card(&pool, owner, "shared-word", "чужое слово", None)
            .await
            .unwrap();
        let card_id = queries::card_by_id(&pool, owner, queries::all_cards(&pool, owner).await.unwrap()[0].id)
            .await
            .unwrap()
            .id;

        // Одинаковое слово у другого профиля — отдельная карточка.
        assert!(queries::insert_card(&pool, stranger, "shared-word", "моё слово", None).await.unwrap());
        assert_eq!(queries::count_cards(&pool, owner).await.unwrap(), 1);
        assert_eq!(queries::count_cards(&pool, stranger).await.unwrap(), 1);

        // Чужую карточку не видно, не удалить и не переписать.
        assert!(matches!(
            queries::card_by_id(&pool, stranger, card_id).await,
            Err(linguarust::error::AppError::NotFound)
        ));
        assert!(matches!(
            queries::delete_card(&pool, stranger, card_id).await,
            Err(linguarust::error::AppError::NotFound)
        ));
        assert!(!queries::update_card(&pool, stranger, card_id, "hacked", "взлом", None).await.unwrap());

        // Повторение чужой карточки тоже недоступно.
        assert!(matches!(
            queries::apply_review(&pool, stranger, card_id, 5, today()).await,
            Err(linguarust::error::AppError::NotFound)
        ));
        assert_eq!(queries::count_cards(&pool, owner).await.unwrap(), 1);

        // Свою карточку владелец править и удалять может.
        assert!(queries::update_card(&pool, owner, card_id, "shared-word", "новый перевод", Some("пример")).await.unwrap());
        let updated = queries::card_by_id(&pool, owner, card_id).await.unwrap();
        assert_eq!(updated.back, "новый перевод");
        assert_eq!(updated.example.as_deref(), Some("пример"));

        queries::delete_card(&pool, owner, card_id).await.unwrap();
        assert!(matches!(
            queries::card_by_id(&pool, owner, card_id).await,
            Err(linguarust::error::AppError::NotFound)
        ));

        cleanup(&pool, owner).await;
        cleanup(&pool, stranger).await;
    });
}

#[test]
fn editing_a_card_keeps_the_sm2_progress() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };

        let profile_id = create_profile(&pool, "integration-edit").await.unwrap();
        queries::insert_card(&pool, profile_id, "edit-word", "перевод", None)
            .await
            .unwrap();
        let card_id = queries::all_cards(&pool, profile_id).await.unwrap()[0].id;

        // Два успешных повторения — карточка «в работе».
        queries::apply_review(&pool, profile_id, card_id, 4, today())
            .await
            .unwrap();
        queries::apply_review(&pool, profile_id, card_id, 4, today())
            .await
            .unwrap();
        let before = queries::card_by_id(&pool, profile_id, card_id).await.unwrap();
        assert_eq!(before.repetitions, 2);

        assert!(
            queries::update_card(&pool, profile_id, card_id, "edited-word", "новый перевод", None)
                .await
                .unwrap()
        );

        let after = queries::card_by_id(&pool, profile_id, card_id).await.unwrap();
        assert_eq!(after.front, "edited-word");
        // Правка текста не должна сбрасывать прогресс по слову.
        assert_eq!(after.repetitions, before.repetitions);
        assert_eq!(after.interval_days, before.interval_days);
        assert_eq!(after.due_date, before.due_date);

        cleanup(&pool, profile_id).await;
    });
}

#[test]
fn invalid_card_input_never_reaches_the_database() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };

        let profile_id = create_profile(&pool, "integration-validation").await.unwrap();

        for (front, back) in [("", "перевод"), ("word", ""), ("   ", "перевод"), ("word", "  ")] {
            let result = queries::insert_card(&pool, profile_id, front, back, None).await;
            assert!(
                matches!(result, Err(linguarust::error::AppError::BadRequest(_))),
                "front={front:?} back={back:?} должен быть отвергнут"
            );
        }

        // Слишком длинные значения отсекаются на границе.
        let long_front = "a".repeat(linguarust::models::MAX_FRONT_LEN + 1);
        let long_back = "b".repeat(linguarust::models::MAX_BACK_LEN + 1);
        assert!(
            queries::insert_card(&pool, profile_id, &long_front, "перевод", None)
                .await
                .is_err()
        );
        assert!(
            queries::insert_card(&pool, profile_id, "word", &long_back, None)
                .await
                .is_err()
        );

        // Обрезка пробелов: в базу попадает чистое значение.
        assert!(
            queries::insert_card(&pool, profile_id, "  spaced  ", "  перевод  ", Some("  пример  "))
                .await
                .unwrap()
        );
        let card = queries::all_cards(&pool, profile_id).await.unwrap().remove(0);
        assert_eq!(card.front, "spaced");
        assert_eq!(card.back, "перевод");
        assert_eq!(card.example.as_deref(), Some("пример"));

        assert_eq!(queries::count_cards(&pool, profile_id).await.unwrap(), 1);

        cleanup(&pool, profile_id).await;
    });
}

#[test]
fn password_is_stored_only_as_an_argon2_hash() {
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
            limiter: Arc::new(ratelimit::RateLimiter::default()),
        });
        let username = format!("u{}", &Uuid::new_v4().to_string()[..8]);
        const PLAINTEXT: &str = "correct-horse-battery";

        let response = app
            .clone()
            .oneshot(form_request(
                "/register",
                format!("username={username}&password={PLAINTEXT}&password2={PLAINTEXT}"),
                None,
            ))
            .await
            .expect("роутер не ответил");
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let cookie = session_cookie(&response);

        // В базе лежит только хеш, и он в формате Argon2id.
        let profile_id: Uuid = sqlx::query_scalar("SELECT id FROM profiles WHERE username = $1")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
        let stored: String =
            sqlx::query_scalar("SELECT password_hash FROM profiles WHERE id = $1")
                .bind(profile_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(stored.starts_with("$argon2id$"), "неверный формат: {stored}");
        assert!(!stored.contains(PLAINTEXT), "в базе лежит открытый пароль");

        // Ни одна страница профиля не отдаёт хеш.
        for path in ["/account", "/cards", "/", "/stats"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header(header::COOKIE, &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .expect("роутер не ответил");
            let status = response.status();
            let body = response
                .into_body()
                .collect()
                .await
                .expect("тело ответа читается")
                .to_bytes();
            let body = String::from_utf8_lossy(&body);
            assert!(
                !body.contains(PLAINTEXT) && !body.contains("argon2") && !body.contains("$argon2"),
                "{path} выдал пароль или хеш"
            );
            assert_eq!(status, StatusCode::OK, "{path} вернул {status}");
        }

        // Cookie сессии — непрозрачный UUID, пароля в ней нет.
        assert!(cookie.contains("lr_session="));
        assert!(!cookie.contains(PLAINTEXT));

        cleanup(&pool, profile_id).await;
    });
}

#[test]
fn a_deck_can_be_imported_and_exported_back() {
    runtime().block_on(async {
        let Some(pool) = init_pool().await else {
            eprintln!("DATABASE_URL не задан — интеграционные тесты пропущены");
            return;
        };

        let profile_id = create_profile(&pool, "integration-deck").await.unwrap();

        // Anki-файл: три служебные колонки, потом слово, перевод и пример.
        let mut file = String::new();
        file.push_str("#separator:tab\n");
        file.push_str("#notetype column 1\n");
        file.push_str("#deck column 2\n");
        file.push_str("#tags column 3\n");
        file.push_str("Basic\tDefault\tдедлайн\tdeadline\tсрок сдачи\tдо пятницы\n");
        file.push_str("Basic\tDefault\t\tprofit\tприбыль\t\n");
        file.push_str("Basic\tDefault\t\tbroken\t\t\n");

        let deck = linguarust::anki::parse(&file);
        assert_eq!(deck.cards.len(), 3);

        let outcome = queries::import_cards(&pool, profile_id, &deck.cards)
            .await
            .unwrap();
        assert_eq!(outcome.added, 2, "строка без перевода не импортируется");
        assert_eq!(outcome.invalid, 1, "пустой перевод отвергается проверкой");
        assert_eq!(outcome.duplicates, 0);

        // Повторный импорт того же файла не создаёт дублей.
        let again = queries::import_cards(&pool, profile_id, &deck.cards)
            .await
            .unwrap();
        assert_eq!(again.added, 0);
        assert_eq!(again.duplicates, 2);
        assert_eq!(queries::count_cards(&pool, profile_id).await.unwrap(), 2);

        // Выгрузка читается обратно тем же разбором.
        let exported = queries::export_entries(&pool, profile_id).await.unwrap();
        assert_eq!(exported.len(), 2);
        let restored = linguarust::anki::parse(&linguarust::anki::export(&exported));
        let summary = linguarust::anki::report(&restored);
        assert_eq!(summary.parsed, 2, "выгруженная колода читается обратно");
        let words: Vec<&str> = restored.cards.iter().map(|c| c.front.as_str()).collect();
        assert!(words.contains(&"deadline"), "{words:?}");

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
            limiter: Arc::new(ratelimit::RateLimiter::default()),
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
            limiter: Arc::new(ratelimit::RateLimiter::default()),
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
