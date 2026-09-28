//! Регистрация, вход, выход и страница профиля.

use std::time::Instant;

use axum::Form;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::AppState;
use crate::auth;
use crate::error::{AppError, AppResult};
use crate::routes::{NavContext, html, nav_context, page_impl, urlencode};

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
    // Лимит считаем по паре «адрес + логин»: один человек с одной машины
    // может зарегистрироваться много раз, но перебрать чужие логини — нет.
    let key = rate_key(&form.username);
    state.limiter.allow(&key, Instant::now()).map_err(too_many)?;

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
        Ok(_) => {
            // Успех сбрасывает счётчик: лимит защищает от перебора, а не
            // от нормальной работы.
            state.limiter.forget(&key);
            Ok(Redirect::to("/cards"))
        }
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
    let key = rate_key(&form.username);
    state.limiter.allow(&key, Instant::now()).map_err(too_many)?;

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
        Ok(_) => {
            state.limiter.forget(&key);
            Ok(Redirect::to("/cards"))
        }
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

/// Ключ счётчика попыток: нормализованный логин.
///
/// Ключ по логину, а не по адресу: за обратным прокси (и на Vercel) адрес
/// либо недоступен, либо общий для всех, так что он бесполезен как основание
/// для лимита. Ограничение по логину закрывает основную угрозу — перебор
/// пароля к одному аккаунту; защиту от «веера» логинов должен давать прокси.
fn rate_key(username: &str) -> String {
    username.trim().to_ascii_lowercase()
}

fn too_many(denied: crate::ratelimit::Denied) -> AppError {
    AppError::TooManyRequests(denied.retry_after.as_secs())
}

fn redirect_with_error(path: &str, error: &str, username: &str) -> Redirect {
    Redirect::to(&format!(
        "{path}?error={}&username={}",
        urlencode(error),
        urlencode(username)
    ))
}
