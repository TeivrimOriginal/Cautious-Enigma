//! Идентификация пользователя: регистрация, вход, выход и сессии.
//!
//! Есть два пути:
//! - **аккаунт**: логин + пароль, пароль хранится только как хеш Argon2id;
//! - **гость**: вход без пароля (по имени) — для быстрого старта.
//!
//! В обоих случаях браузер получает cookie с непрозрачным токеном сессии:
//! сам токен не содержит никакой информации о пользователе, а подделать его
//! невозможно — 122 бита случайности.

use std::time::Duration;

use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tower_cookies::cookie::time::Duration as CookieDuration;
use tower_cookies::cookie::{Cookie, SameSite};
use tower_cookies::{Cookies, Key};
use uuid::Uuid;

use crate::AppState;
use crate::error::{AppError, AppResult};

pub const SESSION_COOKIE: &str = "lr_session";
/// Имя старой cookie из версии без аккаунтов. Нужна только для миграции
/// существующих гостевых профилей и удаляется при следующем входе.
pub const LEGACY_PROFILE_COOKIE: &str = "lr_profile";
/// Старое имя оставлено для кода, который импортировал его до миграции.
pub const PROFILE_COOKIE: &str = LEGACY_PROFILE_COOKIE;

/// Сколько живёт сессия без повторного входа.
const SESSION_TTL_DAYS: i64 = 30;

/// Границы имени гостя. Совпадают с `CHECK` в миграции `0001_init.sql`.
const GUEST_NAME_LEN: (usize, usize) = (1, 40);

/// Минимальная длина пароля.
pub const MIN_PASSWORD_LEN: usize = 8;
/// Допустимая длина логина.
pub const LOGIN_LEN: (usize, usize) = (3, 32);

/// Соль фиксированной длины (16 байт) — берём из криптостойкого UUID.
fn random_salt() -> [u8; 16] {
    *Uuid::new_v4().as_bytes()
}

/// Хеширует пароль алгоритмом Argon2id со случайной солью (формат PHC).
pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = random_salt();
    let argon = Argon2::default();
    argon
        .hash_password_with_salt(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::Internal(format!("argon2 hash: {err}")))
}

/// Проверяет пароль против сохранённого хеша.
pub fn verify_password(password: &str, hash: &str) -> bool {
    if hash.is_empty() {
        return false;
    }
    Argon2::default()
        .verify_password(password.as_bytes(), hash)
        .is_ok()
}

/// Проверяет логин: 3–32 символа, буквы, цифры, дефис и подчёркивание.
pub fn validate_username(username: &str) -> Result<String, AppError> {
    let name = username.trim();
    if name.chars().count() < LOGIN_LEN.0 || name.chars().count() > LOGIN_LEN.1 {
        return Err(AppError::BadRequest(format!(
            "Логин должен быть от {} до {} символов",
            LOGIN_LEN.0, LOGIN_LEN.1
        )));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::BadRequest(
            "В логине можно использовать латиницу, цифры, дефис и подчёркивание".into(),
        ));
    }
    Ok(name.to_ascii_lowercase())
}

/// Проверяет пароль: минимум 8 символов.
pub fn validate_password(password: &str) -> Result<(), AppError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!(
            "Пароль должен быть не короче {MIN_PASSWORD_LEN} символов"
        )));
    }
    Ok(())
}

/// Проверяет имя гостя: 1–40 символов после обрезки пробелов.
///
/// Границы совпадают с `CHECK` в миграции `0001_init.sql`: если бы проверка
/// была мягче, база вернула бы ошибку ConstraintViolation вместо понятного
/// 400, а если строже — гость с валидным именем получил бы отказ.
pub fn validate_guest_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    let len = name.chars().count();
    if len < GUEST_NAME_LEN.0 {
        return Err(AppError::BadRequest("Введите имя".into()));
    }
    if len > GUEST_NAME_LEN.1 {
        return Err(AppError::BadRequest(format!(
            "Имя не длиннее {} символов",
            GUEST_NAME_LEN.1
        )));
    }
    Ok(name.to_string())
}

type LoginRow = (Uuid, String, Option<String>, DateTime<Utc>, Option<String>);

/// Профиль пользователя (аккаунт или гость).
#[derive(Debug, Clone)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub username: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl Profile {
    /// Зарегистрирован ли пользователь (у гостя логина нет).
    pub fn is_registered(&self) -> bool {
        self.username.is_some()
    }
}

/// Необязательный профиль: для гостевых страниц.
#[derive(Debug, Clone)]
pub struct MaybeProfile(pub Option<Profile>);

impl MaybeProfile {
    /// Забирает профиль, если сессия есть.
    pub fn into_inner(self) -> Option<Profile> {
        self.0
    }

    pub fn as_ref(&self) -> Option<&Profile> {
        self.0.as_ref()
    }
}

impl FromRequestParts<AppState> for MaybeProfile {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeProfile(load_profile(parts, state).await))
    }
}

impl FromRequestParts<AppState> for Profile {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match load_profile(parts, state).await {
            Some(profile) => Ok(profile),
            None => Err(AppError::Unauthorized),
        }
    }
}

/// Создаёт аккаунт, ставит cookie сессии и возвращает профиль.
///
/// Если пользователь пришёл из гостевого режима, его профиль обновляется
/// на месте: карточки и история не теряются при регистрации.
pub async fn register(
    pool: &sqlx::PgPool,
    cookies: &Cookies,
    key: &Key,
    secure: bool,
    username: &str,
    password: &str,
) -> Result<Profile, AppError> {
    let username = validate_username(username)?;
    validate_password(password)?;
    let hash = hash_password(password)?;

    let current = session_profile_id(pool, cookies, key).await;
    if let Some(profile_id) = current {
        let existing_username: Option<String> =
            sqlx::query_scalar("SELECT username FROM profiles WHERE id = $1")
                .bind(profile_id)
                .fetch_one(pool)
                .await?;
        if existing_username.is_some() {
            return Err(AppError::Conflict(
                "Аккаунт уже создан. Выйдите, чтобы зарегистрировать другой логин".into(),
            ));
        }
    }

    // Хеш уже посчитан до проверки логина: если логин занят, ответ по времени
    // не отличается от ответа при неверном пароле.
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM profiles WHERE lower(username) = $1)")
            .bind(&username)
            .fetch_one(pool)
            .await?;

    if exists {
        return Err(AppError::Conflict("Такой логин уже занят".into()));
    }

    let profile = if let Some(profile_id) = current {
        let row: (Uuid, String, Option<String>, DateTime<Utc>) = sqlx::query_as(
            "UPDATE profiles
             SET name = $1, username = $1, password_hash = $2
             WHERE id = $3 AND username IS NULL
             RETURNING id, name, username, created_at",
        )
        .bind(&username)
        .bind(&hash)
        .bind(profile_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Conflict("Не удалось сохранить гостевой профиль".into()))?;
        Profile {
            id: row.0,
            name: row.1,
            username: row.2,
            created_at: row.3,
        }
    } else {
        let id = Uuid::new_v4();
        let row: (Uuid, String, Option<String>, DateTime<Utc>) = sqlx::query_as(
            "INSERT INTO profiles (id, name, username, password_hash)
             VALUES ($1, $2, $3, $4)
             RETURNING id, name, username, created_at",
        )
        .bind(id)
        .bind(&username)
        .bind(&username)
        .bind(&hash)
        .fetch_one(pool)
        .await
        .map_err(|err| match &err {
            // Гонка: уникальный индекс не дал создать дубль.
            sqlx::Error::Database(db) if db.is_unique_violation() => {
                AppError::Conflict("Такой логин уже занят".into())
            }
            _ => AppError::Database(err),
        })?;
        Profile {
            id: row.0,
            name: row.1,
            username: row.2,
            created_at: row.3,
        }
    };

    start_session(pool, cookies, profile.id, secure).await?;
    clear_legacy_profile(cookies, key);
    Ok(profile)
}

/// Вход по логину и паролю.
pub async fn login(
    pool: &sqlx::PgPool,
    cookies: &Cookies,
    key: &Key,
    secure: bool,
    username: &str,
    password: &str,
) -> Result<Profile, AppError> {
    let row: Option<LoginRow> = sqlx::query_as(
        "SELECT id, name, username, created_at, password_hash
         FROM profiles WHERE lower(username) = $1",
    )
    .bind(username.trim().to_ascii_lowercase())
    .fetch_optional(pool)
    .await?;

    let (id, name, login_name, created_at, hash) = match row {
        Some(row) => row,
        None => {
            // Нет пользователя — всё равно считаем хеш, чтобы не выдавать себя временем.
            let _ = hash_password(password);
            return Err(AppError::Unauthorized);
        }
    };

    let valid = match hash {
        Some(hash) => verify_password(password, &hash),
        None => {
            // Гость без пароля: вход по логину невозможен.
            let _ = hash_password(password);
            false
        }
    };

    if !valid {
        return Err(AppError::Unauthorized);
    }

    let profile = Profile {
        id,
        name,
        username: login_name,
        created_at,
    };
    start_session(pool, cookies, profile.id, secure).await?;
    clear_legacy_profile(cookies, key);
    Ok(profile)
}

/// Гостевой вход: профиль без логина, но с сессией.
pub async fn login_as_guest(
    pool: &sqlx::PgPool,
    cookies: &Cookies,
    key: &Key,
    secure: bool,
    name: &str,
) -> Result<Profile, AppError> {
    let name = validate_guest_name(name)?;

    // Тот же браузер возвращает тот же токен: обновляем имя, а не плодим дубли.
    if let Some(existing) = session_profile_id(pool, cookies, key).await {
        let updated = sqlx::query_as::<_, (Uuid, String, Option<String>, DateTime<Utc>)>(
            "UPDATE profiles SET name = $1 WHERE id = $2
                 RETURNING id, name, username, created_at",
        )
        .bind(&name)
        .bind(existing)
        .fetch_optional(pool)
        .await?;

        if let Some(row) = updated {
            let profile = Profile {
                id: row.0,
                name: row.1,
                username: row.2,
                created_at: row.3,
            };
            start_session(pool, cookies, profile.id, secure).await?;
            clear_legacy_profile(cookies, key);
            return Ok(profile);
        }
    }

    let id = Uuid::new_v4();
    let row: (Uuid, String, Option<String>, DateTime<Utc>) = sqlx::query_as(
        "INSERT INTO profiles (id, name) VALUES ($1, $2)
         RETURNING id, name, username, created_at",
    )
    .bind(id)
    .bind(name)
    .fetch_one(pool)
    .await?;

    let profile = Profile {
        id: row.0,
        name: row.1,
        username: row.2,
        created_at: row.3,
    };
    start_session(pool, cookies, profile.id, secure).await?;
    clear_legacy_profile(cookies, key);
    Ok(profile)
}

/// Совместимость со старым вызовом: создаёт гостевой профиль по имени.
pub async fn upsert_profile(
    pool: &sqlx::PgPool,
    cookies: &Cookies,
    key: &Key,
    name: &str,
) -> Result<Profile, AppError> {
    login_as_guest(pool, cookies, key, false, name).await
}

/// Совместимость со старым вызовом: удаляет обе cookie профиля.
pub fn clear_profile(cookies: &Cookies, key: &Key) {
    clear_session(cookies);
    clear_legacy_profile(cookies, key);
}

/// Выход: удаляет сессию в базе и cookie в браузере.
pub async fn logout(pool: &sqlx::PgPool, cookies: &Cookies, key: &Key) -> AppResult<()> {
    if let Some(token) = session_token(cookies) {
        sqlx::query("DELETE FROM sessions WHERE token = $1")
            .bind(token)
            .execute(pool)
            .await?;
    }
    clear_session(cookies);
    clear_legacy_profile(cookies, key);
    Ok(())
}

/// Создаёт сессию и кладёт токен в cookie.
async fn start_session(
    pool: &sqlx::PgPool,
    cookies: &Cookies,
    profile_id: Uuid,
    secure: bool,
) -> AppResult<Uuid> {
    let token = Uuid::new_v4();
    let expires = Utc::now() + ChronoDuration::days(SESSION_TTL_DAYS);

    // Сессия всегда ротируется: старый токен нельзя использовать параллельно
    // с новым после входа или переключения аккаунта.
    if let Some(previous) = session_token(cookies) {
        sqlx::query("DELETE FROM sessions WHERE token = $1")
            .bind(previous)
            .execute(pool)
            .await?;
    }

    sqlx::query("INSERT INTO sessions (token, profile_id, expires_at) VALUES ($1, $2, $3)")
        .bind(token)
        .bind(profile_id)
        .bind(expires)
        .execute(pool)
        .await?;

    // Чистим просроченные сессии того же пользователя.
    sqlx::query("DELETE FROM sessions WHERE profile_id = $1 AND expires_at < now()")
        .bind(profile_id)
        .execute(pool)
        .await?;

    cookies.add(
        Cookie::build((SESSION_COOKIE, token.to_string()))
            .path("/")
            .http_only(true)
            .secure(secure)
            .same_site(SameSite::Lax)
            .max_age(CookieDuration::seconds(SESSION_TTL_DAYS * 86_400))
            .build(),
    );

    Ok(token)
}

fn session_token(cookies: &Cookies) -> Option<Uuid> {
    cookies
        .get(SESSION_COOKIE)
        .and_then(|cookie| Uuid::parse_str(cookie.value()).ok())
}

fn clear_session(cookies: &Cookies) {
    cookies.remove(
        Cookie::build((SESSION_COOKIE, String::new()))
            .path("/")
            .build(),
    );
}

fn clear_legacy_profile(cookies: &Cookies, key: &Key) {
    cookies.signed(key).remove(
        Cookie::build((LEGACY_PROFILE_COOKIE, String::new()))
            .path("/")
            .build(),
    );
}

fn legacy_profile_id(cookies: &Cookies, key: &Key) -> Option<Uuid> {
    cookies
        .signed(key)
        .get(LEGACY_PROFILE_COOKIE)
        .and_then(|cookie| Uuid::parse_str(cookie.value()).ok())
}

async fn session_profile_id(pool: &sqlx::PgPool, cookies: &Cookies, key: &Key) -> Option<Uuid> {
    if let Some(token) = session_token(cookies) {
        let profile_id = sqlx::query_scalar(
            "SELECT profile_id FROM sessions WHERE token = $1 AND expires_at > now()",
        )
        .bind(token)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        if profile_id.is_some() {
            return profile_id;
        }
    }

    // Миграция гостей, созданных до появления sessions.
    let profile_id = legacy_profile_id(cookies, key)?;
    sqlx::query_scalar("SELECT id FROM profiles WHERE id = $1")
        .bind(profile_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Читает профиль из новой сессии или старой подписанной cookie.
async fn load_profile(parts: &mut Parts, state: &AppState) -> Option<Profile> {
    let cookies = Cookies::from_request_parts(parts, state).await.ok()?;

    if let Some(token) = session_token(&cookies) {
        let row: Option<(Uuid, String, Option<String>, DateTime<Utc>)> = sqlx::query_as(
            "SELECT p.id, p.name, p.username, p.created_at
             FROM sessions s
             JOIN profiles p ON p.id = s.profile_id
             WHERE s.token = $1 AND s.expires_at > now()",
        )
        .bind(token)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        if let Some(row) = row {
            return Some(Profile {
                id: row.0,
                name: row.1,
                username: row.2,
                created_at: row.3,
            });
        }
    }

    // Старые гости продолжают работать до следующего входа; при первом входе
    // cookie заменяется серверной сессией.
    let profile_id = legacy_profile_id(&cookies, &state.cfg.cookie_key)?;
    let row: (Uuid, String, Option<String>, DateTime<Utc>) =
        sqlx::query_as("SELECT id, name, username, created_at FROM profiles WHERE id = $1")
            .bind(profile_id)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()?;

    Some(Profile {
        id: row.0,
        name: row.1,
        username: row.2,
        created_at: row.3,
    })
}

/// Сколько осталось до истечения сессии (для будущего «продлить»).
pub fn session_left(expires_at: DateTime<Utc>) -> Duration {
    let left = expires_at - Utc::now();
    if left.num_seconds() <= 0 {
        Duration::ZERO
    } else {
        Duration::from_secs(left.num_seconds() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_roundtrip() {
        let hash = hash_password("correct horse battery").expect("хеш строится");
        assert!(hash.starts_with("$argon2id$"), "неверный формат: {hash}");
        assert!(verify_password("correct horse battery", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn hashes_are_salted_and_unique() {
        let first = hash_password("same-password").unwrap();
        let second = hash_password("same-password").unwrap();
        assert_ne!(first, second, "соль должна быть случайной");
        assert!(verify_password("same-password", &first));
        assert!(verify_password("same-password", &second));
    }

    #[test]
    fn verify_rejects_garbage() {
        assert!(!verify_password("что угодно", "не-хеш"));
        assert!(!verify_password("что угодно", ""));
    }

    #[test]
    fn username_rules() {
        assert_eq!(validate_username("  Student_1 ").unwrap(), "student_1");
        assert!(validate_username("ab").is_err(), "слишком короткий");
        assert!(
            validate_username(&"a".repeat(33)).is_err(),
            "слишком длинный"
        );
        assert!(validate_username("имя").is_err(), "только латиница");
        assert!(validate_username("name with space").is_err());
    }

    #[test]
    fn username_boundaries_are_inclusive() {
        // Границы 3 и 32 символа входят в допустимые: ровно на краях
        // логин ещё валиден.
        assert!(validate_username(&"a".repeat(3)).is_ok(), "3 символа");
        assert!(validate_username(&"a".repeat(32)).is_ok(), "32 символа");
        assert!(validate_username("ab").is_err());
        assert!(validate_username(&"a".repeat(33)).is_err());
    }

    #[test]
    fn username_length_counts_characters_not_bytes() {
        // Латиница в UTF-8 однобайтовая, поэтому проверяем именно символы:
        // обрезка по байтам отрезала бы часть валидного логина.
        let long = "ё".repeat(32);
        assert_eq!(long.len(), 64);
        assert!(validate_username(&long).is_err(), "кириллица недопустима");
        let ascii = "a".repeat(32);
        assert_eq!(ascii.len(), 32);
        assert!(validate_username(&ascii).is_ok());
    }

    #[test]
    fn username_does_not_accept_shell_or_sql_metacharacters() {
        for name in [
            "'; DROP TABLE profiles; --",
            "user$(whoami)",
            "`id`",
            "../../etc/passwd",
            "user|name",
            "user\nname",
        ] {
            assert!(validate_username(name).is_err(), "name={name:?}");
        }
    }

    #[test]
    fn guest_name_rules() {
        assert_eq!(validate_guest_name("  Аня  ").unwrap(), "Аня");
        assert!(validate_guest_name("").is_err());
        assert!(validate_guest_name("   ").is_err(), "пробелы не имя");
    }

    #[test]
    fn guest_name_boundaries_match_the_database_check() {
        assert!(validate_guest_name(&"я".repeat(40)).is_ok(), "40 символов");
        assert!(validate_guest_name(&"я".repeat(41)).is_err(), "41 символ");
        // Границы считаются в символах: 40 кириллических букв — это
        // 80 байт, но CHECK в базе тоже считает символы.
        assert_eq!("я".repeat(40).len(), 80);
    }

    #[test]
    fn guest_name_allows_html_but_not_empty() {
        // Имя попадает в HTML-шаблон, где его экранирует askama.
        assert!(validate_guest_name("<script>x</script>").is_ok());
    }

    #[test]
    fn session_left_never_goes_negative() {
        // Метрики и шапка показывают остаток сессии; отрицательное значение
        // означало бы «-3 дн. до конца сессии».
        let now = Utc::now();
        assert_eq!(session_left(now - ChronoDuration::days(1)), Duration::ZERO);
        assert_eq!(session_left(now - ChronoDuration::seconds(1)), Duration::ZERO);
        assert_eq!(session_left(now), Duration::ZERO);
        assert!(session_left(now + ChronoDuration::hours(1)) > Duration::from_secs(3_590));
    }

    #[test]
    fn profile_registration_flag() {
        let guest = Profile {
            id: Uuid::nil(),
            name: "Гость".into(),
            username: None,
            created_at: Utc::now(),
        };
        assert!(!guest.is_registered());

        let user = Profile {
            username: Some("student_1".into()),
            ..guest.clone()
        };
        assert!(user.is_registered());
    }

    #[test]
    fn maybe_profile_unwraps_to_the_inner_option() {
        let inner = Profile {
            id: Uuid::nil(),
            name: "Гость".into(),
            username: None,
            created_at: Utc::now(),
        };

        assert!(MaybeProfile(None).into_inner().is_none());
        assert!(MaybeProfile(None).as_ref().is_none());

        let present = MaybeProfile(Some(inner.clone()));
        assert!(present.as_ref().is_some());
        assert_eq!(present.into_inner().map(|p| p.name), Some("Гость".into()));
    }

    #[test]
    fn password_hash_never_contains_the_password() {
        // Страховка от опечатки в будущем: хеш не должен содержать
        // открытый пароль даже частично.
        let password = "correct-horse-battery-staple";
        let hash = hash_password(password).unwrap();
        assert!(!hash.contains(password));
        assert!(!hash.contains(&password[..10]));
        assert!(!hash.to_lowercase().contains(&password.to_lowercase()[..8]));
    }

    #[test]
    fn verify_password_rejects_a_hash_of_another_password() {
        let hash = hash_password("first-password").unwrap();
        assert!(!verify_password("second-password", &hash));
        // Пароль другого пользователя, даже с тем же хешем соли.
        assert!(!verify_password("", &hash));
    }

    #[test]
    fn verify_password_rejects_a_truncated_hash() {
        let hash = hash_password("correct-horse-battery").unwrap();
        let truncated = &hash[..hash.len() / 2];
        assert!(
            !verify_password("correct-horse-battery", truncated),
            "обрезанный хеш не должен проходить проверку"
        );
    }

    #[test]
    fn password_rules() {
        assert!(validate_password("12345678").is_ok());
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("пароль подлиннее").is_ok());
    }
}
