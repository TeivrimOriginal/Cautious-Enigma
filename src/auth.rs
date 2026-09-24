//! Идентификация пользователя без паролей: профиль хранится в подписанной cookie.
//!
//! Cookie содержит только UUID профиля и подписана ключом из конфигурации,
//! поэтому подделать чужой идентификатор на клиенте невозможно.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use chrono::{DateTime, Utc};
use tower_cookies::cookie::time::Duration as CookieDuration;
use tower_cookies::cookie::{Cookie, SameSite};
use tower_cookies::{Cookies, Key};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppError;

pub const PROFILE_COOKIE: &str = "lr_profile";
const PROFILE_MAX_AGE: CookieDuration = CookieDuration::seconds(60 * 60 * 24 * 365);

/// Профиль текущего пользователя.
#[derive(Debug, Clone)]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

/// Необязательный профиль: для страниц, которые показывают экран приветствия.
#[derive(Debug, Clone)]
pub struct MaybeProfile(pub Option<Profile>);

/// Читает профиль из cookie, если она есть и валидна.
impl FromRequestParts<AppState> for MaybeProfile {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(MaybeProfile(load_profile(parts, state).await))
    }
}

/// Обязательный профиль: используется в обработчиках, изменяющих данные.
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

/// Создаёт или обновляет профиль и кладёт UUID в подписанную cookie.
pub async fn upsert_profile(
    pool: &sqlx::PgPool,
    cookies: &Cookies,
    key: &Key,
    name: &str,
) -> Result<Profile, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Введите имя".into()));
    }
    if name.chars().count() > 40 {
        return Err(AppError::BadRequest("Имя не длиннее 40 символов".into()));
    }

    // Один и тот же браузер возвращает тот же cookie: обновляем имя,
    // а не плодим дубликаты профилей.
    let existing = cookies
        .signed(key)
        .get(PROFILE_COOKIE)
        .and_then(|cookie| Uuid::parse_str(cookie.value()).ok());

    let profile = match existing {
        Some(id) => {
            let updated = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
                "UPDATE profiles SET name = $1 WHERE id = $2
                 RETURNING id, name, created_at",
            )
            .bind(name)
            .bind(id)
            .fetch_optional(pool)
            .await?;
            match updated {
                Some(row) => Profile {
                    id: row.0,
                    name: row.1,
                    created_at: row.2,
                },
                None => insert_profile(pool, name).await?,
            }
        }
        None => insert_profile(pool, name).await?,
    };

    cookies.signed(key).add(
        Cookie::build((PROFILE_COOKIE, profile.id.to_string()))
            .path("/")
            .http_only(true)
            .same_site(SameSite::Lax)
            .max_age(PROFILE_MAX_AGE)
            .build(),
    );

    Ok(profile)
}

/// Удаляет cookie профиля (выход «на чистый лист»).
pub fn clear_profile(cookies: &Cookies, key: &Key) {
    cookies.signed(key).remove(
        Cookie::build((PROFILE_COOKIE, String::new()))
            .path("/")
            .build(),
    );
}

async fn insert_profile(pool: &sqlx::PgPool, name: &str) -> Result<Profile, AppError> {
    let row = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
        "INSERT INTO profiles (id, name) VALUES ($1, $2) RETURNING id, name, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .fetch_one(pool)
    .await?;

    Ok(Profile {
        id: row.0,
        name: row.1,
        created_at: row.2,
    })
}

async fn load_profile(parts: &mut Parts, state: &AppState) -> Option<Profile> {
    let cookies = Cookies::from_request_parts(parts, state).await.ok()?;
    let id: Uuid = cookies
        .signed(&state.cfg.cookie_key)
        .get(PROFILE_COOKIE)?
        .value()
        .parse()
        .ok()?;

    let row = sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
        "SELECT id, name, created_at FROM profiles WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await
    .ok()??;

    Some(Profile {
        id: row.0,
        name: row.1,
        created_at: row.2,
    })
}
