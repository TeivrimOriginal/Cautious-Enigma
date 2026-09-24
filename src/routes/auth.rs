//! Регистрация, вход, выход и страница профиля.

use axum::Form;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::AppState;
use crate::auth;
use crate::error::{AppError, AppResult};
use crate::routes::{NavContext, html, nav_context, page_impl};

#[derive(Debug, askama::Template)]
#[template(path = "register.html")]
pub struct RegisterPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub error: String,
    pub username: String,
    /// Подсказка по требованиям к паролю.
    pub min_password: usize,
}

page_impl!(RegisterPage {
    error: String,
    username: String,
    min_password: usize,
});

#[derive(Debug, askama::Template)]
#[template(path = "login.html")]
pub struct LoginPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub error: String,
    pub username: String,
}

page_impl!(LoginPage {
    error: String,
    username: String,
});

#[derive(Debug, Default, Deserialize)]
pub struct AuthQuery {
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterForm {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub password2: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginForm {
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct GuestForm {
    #[serde(default)]
    pub name: String,
}

/// `GET /register` — форма создания аккаунта.
pub async fn register_form(
    State(state): State<AppState>,
    auth::MaybeProfile(profile): auth::MaybeProfile,
    Query(query): Query<AuthQuery>,
) -> AppResult<Response> {
    // Зарегистрированный пользователь уже внутри приложения. Гость может
    // перейти на страницу регистрации и сохранить свой прогресс.
    if profile
        .as_ref()
        .is_some_and(|profile| profile.is_registered())
    {
        return Ok(Redirect::to("/cards").into_response());
    }

    let nav = nav_context(&state, profile.as_ref()).await?;
    let mut page = RegisterPage::new(nav, "Регистрация", "register");
    page.error = query.error.unwrap_or_default();
    page.username = query.username.unwrap_or_default();
    page.min_password = auth::MIN_PASSWORD_LEN;
    Ok(html(page)?.into_response())
}

/// `POST /register` — создаёт аккаунт и сразу открывает сессию.
pub async fn register(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<RegisterForm>,
) -> AppResult<Redirect> {
    if form.password != form.password2 {
        return Ok(redirect_with_error(
            "/register",
            "Пароли не совпадают",
            &form.username,
        ));
    }

    match auth::register(
        &state.db,
        &cookies,
        &state.cfg.cookie_key,
        state.cfg.is_production,
        &form.username,
        &form.password,
    )
    .await
    {
        Ok(_) => Ok(Redirect::to("/cards")),
        Err(AppError::Conflict(message) | AppError::BadRequest(message)) => {
            Ok(redirect_with_error("/register", &message, &form.username))
        }
        Err(err) => Err(err),
    }
}

/// `GET /login` — форма входа в аккаунт.
pub async fn login_form(
    State(state): State<AppState>,
    auth::MaybeProfile(profile): auth::MaybeProfile,
    Query(query): Query<AuthQuery>,
) -> AppResult<Response> {
    if profile
        .as_ref()
        .is_some_and(|profile| profile.is_registered())
    {
        return Ok(Redirect::to("/cards").into_response());
    }

    let nav = nav_context(&state, profile.as_ref()).await?;
    let mut page = LoginPage::new(nav, "Вход", "login");
    page.error = query.error.unwrap_or_default();
    page.username = query.username.unwrap_or_default();
    Ok(html(page)?.into_response())
}

/// `POST /login` — проверяет пароль и создаёт сессию.
pub async fn login(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<LoginForm>,
) -> AppResult<Redirect> {
    match auth::login(
        &state.db,
        &cookies,
        &state.cfg.cookie_key,
        state.cfg.is_production,
        &form.username,
        &form.password,
    )
    .await
    {
        Ok(_) => Ok(Redirect::to("/cards")),
        Err(AppError::Unauthorized) => Ok(redirect_with_error(
            "/login",
            "Неверный логин или пароль",
            &form.username,
        )),
        Err(err) => Err(err),
    }
}

/// `POST /logout` — удаляет серверную сессию и cookie.
pub async fn logout(State(state): State<AppState>, cookies: Cookies) -> AppResult<Redirect> {
    auth::logout(&state.db, &cookies, &state.cfg.cookie_key).await?;
    Ok(Redirect::to("/"))
}

/// `POST /guest` — быстрый вход по имени без пароля.
pub async fn guest(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<GuestForm>,
) -> AppResult<Redirect> {
    auth::login_as_guest(
        &state.db,
        &cookies,
        &state.cfg.cookie_key,
        state.cfg.is_production,
        &form.name,
    )
    .await?;
    Ok(Redirect::to("/cards"))
}

/// `GET /account` — краткая сводка текущего профиля.
pub async fn account(
    State(state): State<AppState>,
    auth::MaybeProfile(profile): auth::MaybeProfile,
) -> AppResult<Response> {
    let Some(profile) = profile else {
        return Err(AppError::Unauthorized);
    };
    let nav = nav_context(&state, Some(&profile)).await?;
    let summary = crate::queries::review_summary(&state.db, profile.id)
        .await
        .ok();

    Ok(html(AccountPage {
        title: "Профиль".into(),
        css: crate::STYLE_CSS,
        js: crate::APP_JS,
        nav,
        current: "account",
        name: profile.name.clone(),
        registered: profile.is_registered(),
        created_at: profile.created_at.format("%d.%m.%Y").to_string(),
        summary,
    })?
    .into_response())
}

#[derive(Debug, askama::Template)]
#[template(path = "account.html")]
pub struct AccountPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub name: String,
    pub registered: bool,
    pub created_at: String,
    pub summary: Option<crate::models::ReviewSummary>,
}

fn redirect_with_error(path: &str, error: &str, username: &str) -> Redirect {
    Redirect::to(&format!(
        "{path}?error={}&username={}",
        urlencode(error),
        urlencode(username)
    ))
}

/// Минимальное URL-кодирование (для сообщений об ошибках).
fn urlencode(value: &str) -> String {
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
}
