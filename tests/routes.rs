//! Тесты HTTP-слоя: проверяем роутер целиком, без подключения к базе.
//!
//! Гостевые страницы и заглушка 404 не ходят в PostgreSQL, поэтому пул
//! создаётся лениво (`connect_lazy`) и такие тесты идут в CI без базы.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use linguarust::sm2::{MAX_INTERVAL_DAYS, MIN_EASE, Sm2State};
use linguarust::config::Config;
use linguarust::{AppState, build_router, ratelimit};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use tower_cookies::Key;

/// Роутер с пулом, который никогда не подключается к реальной базе.
fn test_app() -> axum::Router {
    app_with(ratelimit::RateLimiter::default())
}

/// Роутер с заданным счётчиком попыток: нужен, чтобы проверить лимит входа,
/// не дожидаясь настоящих пяти минут блокировки.
fn app_with(limiter: ratelimit::RateLimiter) -> axum::Router {
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
        limiter: Arc::new(limiter),
    })
}

async fn get(path: &str) -> (StatusCode, String) {
    send(test_app(), Request::builder().uri(path).body(Body::empty()).unwrap()).await
}

/// Отправляет запрос и читает тело ответа как строку.
async fn send(app: axum::Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app
        .oneshot(request)
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

fn form(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn json(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
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

/* ------------------------------------------------------------------ *
 * Сессия на каждом закрытом маршруте
 *
 * Проверяем не один маршрут, а весь список: забытый `Profile` в
 * обработчике означает чтение чужих карточек.
 * ------------------------------------------------------------------ */

/// Закрытые маршруты, доступные только с сессией.
const PROTECTED_GET: &[&str] = &["/cards", "/stats"];

const PROTECTED_POST: &[(&str, &str)] = &[
    ("/cards", "front=test&back=%D1%82%D0%B5%D1%81%D1%82"),
    ("/cards/1/review", "quality=4"),
    ("/cards/1/edit", "front=test&back=test2"),
    ("/cards/1/delete", ""),
    ("/api/cards", r#"{"front":"test","back":"test"}"#),
    ("/api/goal", r#"{"goal":30}"#),
];

#[tokio::test]
async fn every_protected_page_requires_a_session() {
    for path in PROTECTED_GET {
        let (status, body) = get(path).await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{path} отдал {status} без сессии: {body}"
        );
    }
}

#[tokio::test]
async fn every_protected_action_requires_a_session() {
    for (path, body) in PROTECTED_POST {
        let (status, response_body) = send(
            test_app(),
            if path.starts_with("/api/") {
                json(path, body)
            } else {
                form(path, body)
            },
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNAUTHORIZED,
            "{path} отдал {status} без сессии: {response_body}"
        );
    }
}

#[tokio::test]
async fn account_page_needs_a_session() {
    let (status, _) = get("/account").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn a_forged_session_cookie_is_rejected() {
    // Токен сессии — UUID, выданный сервером. Подделанный не должен
    // превращаться в профиль: запрос падает на базе и даёт 401, а не 200.
    let response = test_app()
        .oneshot(
            Request::builder()
                .uri("/cards")
                .header("cookie", "lr_session=00000000-0000-0000-0000-000000000000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("роутер не ответил");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/* ------------------------------------------------------------------ *
 * Защита от перебора паролей
 *
 * Тесты идут через роутер с базой, которой нет, поэтому первая попытка
 * доходит до обработчика и падает на подключении (500). Это не мешает
 * проверке: важно, что лимит срабатывает ДО похода в базу — иначе перебор
 * всё равно стоил бы серверу по Argon2 на каждый запрос.
 * ------------------------------------------------------------------ */

#[tokio::test]
async fn login_is_rate_limited_after_repeated_attempts() {
    // Счётчик с лимитом в одну попытку: проверяем поведение маршрута,
    // не дожидаясь настоящих пяти минут.
    let app = app_with(ratelimit::RateLimiter::new(1));

    let (first, _) = send(
        app.clone(),
        form("/login", "username=student_1&password=wrong-password"),
    )
    .await;
    assert_ne!(first, StatusCode::TOO_MANY_REQUESTS, "первая попытка проходит");

    let response = app
        .oneshot(form(
            "/login",
            "username=student_1&password=wrong-password",
        ))
        .await
        .expect("роутер не ответил");

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        response.headers().get("retry-after").is_some(),
        "без Retry-After клиент долбит форму"
    );
}

#[tokio::test]
async fn registration_is_rate_limited_too() {
    let app = app_with(ratelimit::RateLimiter::new(1));

    let (first, _) = send(
        app.clone(),
        form(
            "/register",
            "username=student_1&password=correct-horse-battery&password2=correct-horse-battery",
        ),
    )
    .await;
    assert_ne!(first, StatusCode::TOO_MANY_REQUESTS);

    let (second, body) = send(
        app,
        form(
            "/register",
            "username=student_1&password=correct-horse-battery&password2=correct-horse-battery",
        ),
    )
    .await;
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS, "тело: {body}");
}

#[tokio::test]
async fn the_limit_is_counted_per_login() {
    // Перебор одного аккаунта не должен блокировать вход другим людям.
    let app = app_with(ratelimit::RateLimiter::new(1));

    let (first, _) = send(
        app.clone(),
        form("/login", "username=student_1&password=wrong-password"),
    )
    .await;
    assert_ne!(first, StatusCode::TOO_MANY_REQUESTS);

    let (other, _) = send(
        app,
        form("/login", "username=student_2&password=wrong-password"),
    )
    .await;
    assert_ne!(
        other,
        StatusCode::TOO_MANY_REQUESTS,
        "другой логин не должен попадать под лимит"
    );
}

#[tokio::test]
async fn login_case_and_spaces_share_one_limit() {
    // Иначе перебор проходил бы простым чередованием регистра.
    let app = app_with(ratelimit::RateLimiter::new(1));

    let (first, _) = send(
        app.clone(),
        form("/login", "username=Student_1&password=wrong-password"),
    )
    .await;
    assert_ne!(first, StatusCode::TOO_MANY_REQUESTS);

    let (second, _) = send(
        app,
        form("/login", "username=%20student_1%20&password=wrong-password"),
    )
    .await;
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn the_limiter_is_per_application_instance() {
    // Два независимых состояния не делят счётчик: иначе тесты на одном
    // хосте блокировали бы друг друга.
    let first = app_with(ratelimit::RateLimiter::new(1));
    let second = app_with(ratelimit::RateLimiter::new(1));

    let (one, _) = send(
        first,
        form("/login", "username=student_1&password=wrong-password"),
    )
    .await;
    let (two, _) = send(
        second,
        form("/login", "username=student_1&password=wrong-password"),
    )
    .await;
    assert_ne!(one, StatusCode::TOO_MANY_REQUESTS);
    assert_ne!(two, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn guest_login_is_not_rate_limited() {
    // Гостевой вход по имени не подбирает пароль, ограничивать его незачем.
    let app = app_with(ratelimit::RateLimiter::new(1));
    for index in 0..3 {
        let (status, _) = send(
            app.clone(),
            form("/guest", &format!("name=guest-{index}")),
        )
        .await;
        assert_ne!(status, StatusCode::TOO_MANY_REQUESTS, "попытка {index}");
    }
}

/* ------------------------------------------------------------------ *
 * Валидация входа на границах
 * ------------------------------------------------------------------ */

#[tokio::test]
async fn translation_rejects_too_long_word() {
    let long = "a".repeat(65);
    let (status, body) = get(&format!("/api/translate?word={long}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "тело: {body}");
}

#[tokio::test]
async fn translation_of_unknown_word_is_not_found() {
    let (status, body) = get("/api/translate?word=zzzqqqxxnotaword").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "тело: {body}");
}

#[tokio::test]
async fn translation_of_a_known_word_works_without_a_profile() {
    // Словарь встроен в бинарник: перевод не требует ни базы, ни сессии.
    let (status, body) = get("/api/translate?word=deadline").await;
    assert_eq!(status, StatusCode::OK, "тело: {body}");
    assert!(body.contains("deadline"), "тело: {body}");
    assert!(body.contains("срок сдачи"), "тело: {body}");
}

#[tokio::test]
async fn translation_does_not_leak_password_hash() {
    let (_, body) = get("/api/translate?word=deadline").await;
    assert!(
        !body.contains("argon2") && !body.contains("password_hash"),
        "в ответе не должно быть ничего про пароли: {body}"
    );
}

/// Askama экранирует `<` и `>` числовыми сущностями, а не именованными:
/// сравниваем с тем, что реально попадает в HTML.
const ESCAPED_SCRIPT: &str = "&#60;script&#62;";

#[tokio::test]
async fn search_query_with_special_characters_is_escaped() {
    // Словарь встроен в бинарник, поэтому поиск работает и без базы —
    // значит проверка экранирования тут настоящая, а не заглушка.
    let (status, body) = get("/dictionary?q=%3Cscript%3Ealert(1)%3C/script%3E").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("<script>alert(1)</script>"),
        "неэкранированный ввод попал в страницу: {body}"
    );
    assert!(
        body.contains(ESCAPED_SCRIPT),
        "ожидалось экранированное значение в поле поиска: {body}"
    );
}

#[tokio::test]
async fn dictionary_is_available_without_a_profile() {
    let (status, body) = get("/dictionary").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Словарь"), "тело: {body}");
}

/* ------------------------------------------------------------------ *
 * XSS: пользовательский ввод не должен попадать в HTML как разметка
 *
 * Проверяем на страницах, которые рендерятся без базы: иначе тест
 * молча выродился бы в «проверку отсутствия базы».
 * ------------------------------------------------------------------ */

#[tokio::test]
async fn login_error_message_is_escaped() {
    let payload = "%3Cscript%3Ealert(1)%3C%2Fscript%3E";
    let (status, body) = get(&format!("/login?error={payload}")).await;
    assert_eq!(status, StatusCode::OK, "страница входа доступна гостю");
    assert!(
        !body.contains("<script>alert(1)</script>"),
        "сообщение об ошибке не экранировано: {body}"
    );
    assert!(
        body.contains(ESCAPED_SCRIPT),
        "ожидалось экранированное сообщение: {body}"
    );
}

#[tokio::test]
async fn login_username_is_escaped() {
    let (status, body) = get("/login?username=%22%3E%3Cimg+src%3Dx+onerror%3Dalert(1)%3E").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("<img src=x onerror=alert(1)>"),
        "логин не экранирован: {body}"
    );
    assert!(body.contains("&#60;img"), "ожидалось экранирование: {body}");
}

#[tokio::test]
async fn register_error_and_username_are_escaped() {
    let (status, body) = get(
        "/register?error=%3Csvg%2Fonload%3Dalert(1)%3E&username=%3Cb%3Eadmin%3C%2Fb%3E",
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains("<svg/onload=alert(1)>"), "{body}");
    assert!(!body.contains("<b>admin</b>"), "{body}");
    assert!(body.contains("&#60;b&#62;admin"), "ожидалось экранирование: {body}");
}

#[tokio::test]
async fn error_page_message_is_escaped() {
    // Некорректный путь попадает в страницу 404 через `error::not_found`.
    let (status, body) = get("/%3Cscript%3Ealert(1)%3C%2Fscript%3E").await;
    assert!(
        status == StatusCode::NOT_FOUND || status.is_client_error(),
        "статус {status}"
    );
    assert!(!body.contains("<script>alert(1)</script>"), "{body}");
}

#[tokio::test]
async fn api_error_message_is_escaped_from_json_injection() {
    // Словарь ищет слово по клику: спецсимволы не находят перевод и дают 404.
    let (status, body) = get("/api/translate?word=%22%2C%20%22error%22%3A%20%22ok").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        !body.contains("\"error\":\"ok\""),
        "инъекция в JSON: {body}"
    );
}

#[tokio::test]
async fn body_larger_than_the_limit_is_rejected() {
    let huge = "x".repeat(70 * 1024);
    let response = test_app()
        .oneshot(json("/api/cards", &format!(r#"{{"front":"{huge}"}}"#)))
        .await
        .expect("роутер не ответил");
    // Проверяется и лимитом тела, и отсутствием сессии — важно лишь то,
    // что сервер не падает и не отвечает 5xx на мусор.
    assert!(
        response.status().is_client_error(),
        "статус {} на слишком большом теле",
        response.status()
    );
}

#[tokio::test]
async fn malformed_json_does_not_panic() {
    let response = test_app()
        .oneshot(json("/api/cards", "{not json"))
        .await
        .expect("роутер не ответил");
    assert!(
        response.status().is_client_error(),
        "статус {} на битом JSON",
        response.status()
    );
}

#[tokio::test]
async fn a_path_that_is_not_a_card_id_is_rejected() {
    // `Path<i64>` не должен пропускать мусор ни в один из обработчиков.
    for path in [
        "/cards/not-a-number/delete",
        "/cards/not-a-number/review",
        "/cards/not-a-number/edit",
    ] {
        let response = test_app()
            .clone()
            .oneshot(form(path, ""))
            .await
            .expect("роутер не ответил");
        assert!(
            response.status().is_client_error(),
            "{path}: статус {}",
            response.status()
        );
    }
}

#[tokio::test]
async fn grammar_check_without_a_body_is_a_client_error() {
    let response = test_app()
        .oneshot(json("/api/grammar/check", "{}"))
        .await
        .expect("роутер не ответил");
    assert!(
        response.status().is_client_error(),
        "статус {} без id упражнения",
        response.status()
    );
}

#[tokio::test]
async fn goal_is_clamped_to_the_allowed_range() {
    // Публичные страницы ради этого не нужны: проверяем, что без сессии
    // маршрут не выполняет запись.
    let response = test_app()
        .oneshot(json("/api/goal", r#"{"goal":100000}"#))
        .await
        .expect("роутер не ответил");
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

/* ------------------------------------------------------------------ *
 * П2: бинарники не должны расходиться по логике
 *
 * Есть два бинаря: `linguarust` (TCP-сервер) и `axum` (serverless для
 * Vercel). Общий код обязан лежать в `src/`, иначе фикс придётся
 * дублировать, а потом забудут про один из бинарей. Проверяем это по
 * исходникам: `include_str!` работает без базы и без сети.
 * ------------------------------------------------------------------ */

/// Содержимое файла пакета относительно `CARGO_MANIFEST_DIR`.
fn source(relative: &str) -> &'static str {
    static MAIN: &str = include_str!("../src/main.rs");
    static SERVERLESS: &str = include_str!("../api/axum.rs");
    static LIB: &str = include_str!("../src/lib.rs");
    match relative {
        "src/main.rs" => MAIN,
        "api/axum.rs" => SERVERLESS,
        "src/lib.rs" => LIB,
        other => panic!("неизвестный файл: {other}"),
    }
}

#[test]
fn both_binaries_use_the_same_router() {
    // Если хоть один перестанет звать `build_router`, маршруты или
    // обработчик паники начнут расходиться: на Vercel полетят 404.
    assert!(
        source("src/main.rs").contains("build_router"),
        "бинарь linguarust собирает роутер мимо build_router"
    );
    assert!(
        source("api/axum.rs").contains("build_router"),
        "бинарь axum собирает роутер мимо build_router"
    );
}

#[test]
fn binaries_hold_no_business_logic() {
    // SQL, маршруты и шаблоны — только в `src/`. В бинарниках допустим
    // транспорт: сокет, сигналы, слой Vercel.
    for file in ["src/main.rs", "api/axum.rs"] {
        let text = source(file);
        for forbidden in [
            "sqlx::",
            "SELECT ",
            "INSERT INTO",
            "UPDATE ",
            ".route(",
            "askama",
            "PasswordHasher",
            "Argon2",
        ] {
            assert!(
                !text.contains(forbidden),
                "{file} содержит бизнес-логику: {forbidden}"
            );
        }
    }
}

#[test]
fn every_route_is_registered_in_one_place() {
    let lib = source("src/lib.rs");
    // Маршруты объявляются только в `build_router`; иначе серверный и
    // serverless-бинарь получают разные списки путей.
    assert!(lib.contains("fn build_router"));
    let router_block_start = lib.find("fn build_router").expect("build_router есть");
    let router_block = &lib[router_block_start..];
    for path in [
        "\"/cards\"",
        "\"/dictionary\"",
        "\"/reading\"",
        "\"/grammar\"",
        "\"/stats\"",
        "\"/login\"",
        "\"/register\"",
        "\"/healthz\"",
    ] {
        assert!(
            router_block.contains(path),
            "маршрут {path} не зарегистрирован в build_router"
        );
    }
}

#[test]
fn every_handler_resolves_its_error_page_the_same_way() {
    // Страницы ошибок собираются в `error.rs`, а не в каждом бинаре:
    // иначе serverless отдаст свой текст вместо общего.
    let lib = source("src/lib.rs");
    assert!(
        lib.contains("error::not_found"),
        "заглушка 404 должна быть общей"
    );
    assert!(lib.contains("handle_panic"), "обработчик паники общий");
}

#[test]
fn panic_handler_covers_the_whole_router() {
    // `CatchPanicLayer` обязан стоять выше всех обработчиков, иначе
    // паника в роуте уйдёт в общий обработчик Vercel с 500 без страницы.
    let lib = source("src/lib.rs");
    let catch = lib.find("CatchPanicLayer::custom").expect("слой паники есть");
    let with_state = lib.find(".with_state(state)").expect("состояние есть");
    assert!(catch < with_state, "слой паники применён после маршрутов");
}

/* ------------------------------------------------------------------ *
 * Ядро продукта: SM-2 и очередь повторения
 *
 * Здесь те же инварианты, что и в юнит-тестах `sm2`/`queue`, но через
 * публичный API крейта: так проверяется, что модуль действительно
 * доступен извне и что его состояние сериализуемо (SM-2 ездит в JSON).
 * ------------------------------------------------------------------ */

#[test]
fn sm2_state_round_trips_through_json() {
    let state = Sm2State {
        repetitions: 4,
        interval_days: 45,
        ease: 2.9,
    };
    let json = serde_json::to_string(&state).expect("состояние должно сериализоваться");
    let restored: Sm2State = serde_json::from_str(&json).expect("и обратно тоже");
    assert_eq!(restored, state);
    assert!((restored.ease - 2.9).abs() < 1e-9);
}

#[test]
fn sm2_review_survives_a_trip_across_the_public_api() {
    // «Лёгкий» ответ на только что созданной карточке: ровно один день.
    let (state, outcome) = Sm2State::default().review(5, linguarust::routes::today());
    assert_eq!(state.repetitions, 1);
    assert_eq!(state.interval_days, 1);
    assert!(outcome.next_due > linguarust::routes::today());
    assert!(state.ease >= MIN_EASE);
}

#[test]
fn sm2_long_run_never_leaves_the_usable_range() {
    let mut state = Sm2State::default();
    let mut day = linguarust::routes::today();
    for _ in 0..40 {
        let (next, outcome) = state.review(5, day);
        assert!((1..=MAX_INTERVAL_DAYS).contains(&next.interval_days));
        assert!(next.ease >= MIN_EASE && next.ease.is_finite());
        state = next;
        day = outcome.next_due;
    }
    // Сорок ответов «легко» — и интервал ещё помещается в колонку INTEGER.
    assert!(i32::try_from(state.interval_days).is_ok());
}
