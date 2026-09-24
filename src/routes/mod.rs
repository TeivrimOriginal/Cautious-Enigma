//! HTTP-маршруты и общий контекст шаблонов.

pub mod api;
pub mod cards;
pub mod dictionary;
pub mod grammar;
pub mod home;
pub mod reading;
pub mod stats;

use chrono::{NaiveDate, Utc};

use axum::response::Html;

use crate::auth::Profile;
use crate::error::{AppError, AppResult};
use crate::{AppState, queries};

/// Рендерит шаблон Askama в HTML-ответ.
///
/// Шаблоны намеренно рендерятся в строку, а не возвращаются как
/// `Html<MyTemplate>`: так один обработчик может отдавать разные страницы
/// (например, экран приветствия или дашборд).
pub(crate) fn html<T: askama::Template>(page: T) -> AppResult<Html<String>> {
    Ok(Html(page.render().map_err(AppError::Template)?))
}

/// Данные для шапки сайта: имя профиля, стрик и число карточек к повторению.
#[derive(Debug, Clone, Default)]
pub struct NavContext {
    pub name: String,
    pub has_profile: bool,
    pub streak: u32,
    pub due: i64,
}

/// Текущая дата в UTC — единая точка отсчёта для SM-2 и статистики.
pub fn today() -> NaiveDate {
    Utc::now().date_naive()
}

/// Собирает шапку. Для гостя возвращается пустой контекст.
pub async fn nav_context(state: &AppState, profile: Option<&Profile>) -> AppResult<NavContext> {
    let Some(profile) = profile else {
        return Ok(NavContext::default());
    };

    let summary = queries::review_summary(&state.db, profile.id).await?;
    let activity = queries::daily_activity(&state.db, profile.id, 90).await?;
    let days = activity.iter().map(|row| row.day).collect();

    Ok(NavContext {
        name: profile.name.clone(),
        has_profile: true,
        streak: crate::stats::current_streak(&days, today()),
        due: summary.due_today,
    })
}

/// Макро создаёт конструктор и `Default` для страницы.
///
/// Сами структуры с `#[derive(Template)]` объявлены в модулях явно:
/// derive-макросы плохо переносятся внутрь `macro_rules!`.
macro_rules! page_impl {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        impl $name {
            pub fn new(
                nav: NavContext,
                title: impl Into<String>,
                current: &'static str,
            ) -> Self {
                Self {
                    title: title.into(),
                    css: crate::STYLE_CSS,
                    js: crate::APP_JS,
                    nav,
                    current,
                    $( $field: Default::default(), )*
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new(NavContext::default(), "", "")
            }
        }
    };
}

pub(crate) use page_impl;
