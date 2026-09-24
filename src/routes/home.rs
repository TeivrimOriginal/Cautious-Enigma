//! Главная страница: экран приветствия для гостей и дашборд для профиля.

use axum::Form;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect};
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::auth::{self, MaybeProfile};
use crate::error::{AppError, AppResult};
use crate::routes::{NavContext, html, nav_context, page_impl, today};
use crate::{AppState, queries, seed, stats};

#[derive(Debug, askama::Template)]
#[template(path = "welcome.html")]
pub struct Welcome {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub error: String,
    pub name: String,
}

page_impl!(Welcome {
    error: String,
    name: String
});

#[derive(Debug, askama::Template)]
#[template(path = "home.html")]
pub struct Dashboard {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub total_cards: i64,
    pub learned: i64,
    pub new_cards: i64,
    pub due_today: i64,
    pub reviews_today: i64,
    pub total_reviews: i64,
    pub translations: i64,
    pub accuracy: f64,
    pub accuracy_text: String,
    pub longest: u32,
    pub bars: Vec<DayBar>,
    pub word_of_day: Option<(String, String, String)>,
    pub level: u32,
    pub level_title: String,
    pub xp: i32,
    pub xp_in_level: i32,
    pub xp_needed: i32,
    pub xp_percent: u32,
    pub daily_goal: i32,
    pub goal_percent: i32,
    pub goal_done: bool,
}

page_impl!(Dashboard {
    total_cards: i64,
    learned: i64,
    new_cards: i64,
    due_today: i64,
    reviews_today: i64,
    total_reviews: i64,
    translations: i64,
    accuracy: f64,
    accuracy_text: String,
    longest: u32,
    bars: Vec<DayBar>,
    word_of_day: Option<(String, String, String)>,
    level: u32,
    level_title: String,
    xp: i32,
    xp_in_level: i32,
    xp_needed: i32,
    xp_percent: u32,
    daily_goal: i32,
    goal_percent: i32,
    goal_done: bool,
});

/// Столбик графика активности за последние дни.
#[derive(Debug, Clone)]
pub struct DayBar {
    pub label: String,
    pub reviews: i64,
    pub percent: u32,
    pub success_percent: u32,
}

#[derive(Debug, Deserialize)]
pub struct NameForm {
    #[serde(default)]
    pub name: String,
}

pub async fn index(
    State(state): State<AppState>,
    MaybeProfile(profile): MaybeProfile,
) -> AppResult<Html<String>> {
    match profile {
        None => welcome_page(state, String::new()).await,
        Some(profile) => {
            let nav = nav_context(&state, Some(&profile)).await?;
            let summary = queries::review_summary(&state.db, profile.id).await?;
            let activity = queries::daily_activity(&state.db, profile.id, 90).await?;
            let days: std::collections::BTreeSet<_> = activity.iter().map(|row| row.day).collect();

            let mut page = Dashboard::new(nav, "Главная", "home");
            page.total_cards = summary.total_cards;
            page.learned = summary.learned_cards;
            page.new_cards = summary.new_cards;
            page.due_today = summary.due_today;
            page.reviews_today = summary.reviews_today;
            page.total_reviews = summary.total_reviews;
            page.translations = summary.translations;
            page.accuracy =
                stats::accuracy_percent(summary.total_reviews, summary.successful_reviews);
            page.accuracy_text = format!("{:.1}", page.accuracy);
            page.longest = stats::longest_streak(&days);
            page.bars = build_bars(&activity);
            page.word_of_day = seed::word_of_the_day(today()).map(|entry| {
                (
                    entry.front.clone(),
                    entry.back.clone(),
                    entry.example.clone().unwrap_or_default(),
                )
            });

            // Опыт, уровень и дневная цель.
            let totals = queries::xp_totals(&state.db, profile.id).await?;
            let xp = stats::total_xp(
                totals.successful_reviews,
                totals.failed_reviews,
                totals.cards,
                totals.grammar_total,
                totals.grammar_correct,
            );
            let level = stats::level_progress(xp);
            let goal = queries::daily_goal(&state.db, profile.id).await?.max(1);
            let goal_percent =
                ((summary.reviews_today as f64 / goal as f64) * 100.0).round() as i32;

            page.xp = xp;
            page.level = level.level;
            page.level_title = level.title().to_string();
            page.xp_in_level = level.in_level;
            page.xp_needed = level.needed;
            page.xp_percent = level.percent;
            page.daily_goal = goal;
            page.goal_percent = goal_percent.min(100);
            page.goal_done = summary.reviews_today >= goal as i64;

            html(page)
        }
    }
}

/// Отдельная страница приветствия (например, после выхода из профиля).
pub async fn welcome(State(state): State<AppState>) -> AppResult<Html<String>> {
    welcome_page(state, String::new()).await
}

async fn welcome_page(state: AppState, error: String) -> AppResult<Html<String>> {
    let mut page = Welcome::new(NavContext::default(), "LinguaRust", "welcome");
    page.error = error;
    let _ = &state;
    html(page)
}

/// Создаёт профиль по имени и ставит подписанную cookie.
pub async fn create_profile(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<NameForm>,
) -> AppResult<Redirect> {
    auth::upsert_profile(&state.db, &cookies, &state.cfg.cookie_key, &form.name)
        .await
        .map_err(|err| match err {
            AppError::BadRequest(message) => AppError::BadRequest(message),
            other => other,
        })?;

    Ok(Redirect::to("/cards"))
}

/// Забывает профиль: cookie удаляется, история остаётся в базе.
pub async fn reset_profile(State(state): State<AppState>, cookies: Cookies) -> AppResult<Redirect> {
    auth::clear_profile(&cookies, &state.cfg.cookie_key);
    Ok(Redirect::to("/welcome"))
}

/// Проверка живости для платформенного мониторинга.
pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
        .is_ok();

    let status = if db_ok {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };
    let body = serde_json::json!({ "status": if db_ok { "ok" } else { "db unavailable" } });
    (status, axum::Json(body))
}

/// Строит график активности за 14 дней: пропуски заполняются нулями.
fn build_bars(activity: &[crate::models::DailyActivity]) -> Vec<DayBar> {
    let today = today();
    let start = today - chrono::Days::new(13);
    let max = activity
        .iter()
        .map(|row| row.reviews)
        .max()
        .unwrap_or(0)
        .max(1);

    (0..14)
        .map(|offset| {
            let day = start + chrono::Days::new(offset);
            let row = activity.iter().find(|row| row.day == day);
            let reviews = row.map_or(0, |row| row.reviews);
            let successful = row.map_or(0, |row| row.successful);
            DayBar {
                label: day.format("%d.%m").to_string(),
                reviews,
                percent: (reviews * 100 / max) as u32,
                success_percent: (successful * 100 / max) as u32,
            }
        })
        .collect()
}
