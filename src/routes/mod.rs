//! HTTP-маршруты и общий контекст шаблонов.

pub mod api;
pub mod auth;
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

/// Минимальное URL-кодирование для сообщений об ошибках в редиректах.
///
/// Отдельная функция, а не `url::form_urlencoded`, потому что нужен
/// предсказуемый вид `+` вместо `%20`: так сообщения читаются в адресной
/// строке и в тестах.
pub(crate) fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::urlencode;

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(
            urlencode("Пароль не совпадают"),
            "%D0%9F%D0%B0%D1%80%D0%BE%D0%BB%D1%8C+%D0%BD%D0%B5+%D1%81%D0%BE%D0%B2%D0%BF%D0%B0%D0%B4%D0%B0%D1%8E%D1%82"
        );
        assert_eq!(urlencode("student_1"), "student_1");
    }

    #[test]
    fn safe_characters_are_untouched() {
        assert_eq!(urlencode("abcXYZ019-_.~"), "abcXYZ019-_.~");
    }

    #[test]
    fn dangerous_characters_are_escaped() {
        // Без кодирования `<` и `&` сломали бы строку запроса и позволили
        // бы внести произвольные параметры в редирект.
        assert_eq!(urlencode("<script>"), "%3Cscript%3E");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("\"quoted\""), "%22quoted%22");
    }

    #[test]
    fn newline_and_crlf_are_escaped() {
        // Заголовок `Location` с CR/LF — это разрыв ответа.
        assert_eq!(urlencode("a\r\nSet-Cookie: x=1"), "a%0D%0ASet-Cookie%3A+x%3D1");
    }

    #[test]
    fn non_ascii_bytes_are_percent_encoded() {
        assert_eq!(urlencode("щ"), "%D1%89");
        assert_eq!(urlencode(""), "");
    }
}
