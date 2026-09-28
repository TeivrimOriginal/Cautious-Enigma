//! Главная страница: экран приветствия для гостей и дашборд для профиля.

use axum::Form;
use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect};
use chrono::NaiveDate;
use serde::Deserialize;
use tower_cookies::Cookies;

use crate::auth::{self, MaybeProfile};
use crate::error::AppResult;
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

/// Строит график активности за 14 дней: пропуски заполняются нулями.
fn build_bars(activity: &[crate::models::DailyActivity]) -> Vec<DayBar> {
    build_bars_for(activity, today())
}

/// График относительно произвольной «сегодняшней» даты.
///
/// Вынесено отдельно, чтобы поведение (окно в 14 дней, нули в пропусках,
/// нормировка столбиков) можно было проверить без обращения к часам.
fn build_bars_for(activity: &[crate::models::DailyActivity], today: NaiveDate) -> Vec<DayBar> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(ymd: &str) -> NaiveDate {
        NaiveDate::parse_from_str(ymd, "%Y-%m-%d").expect("корректная дата в тесте")
    }

    fn activity(ymd: &str, reviews: i64, successful: i64) -> crate::models::DailyActivity {
        crate::models::DailyActivity {
            day: day(ymd),
            reviews,
            successful,
        }
    }

    #[test]
    fn graph_covers_exactly_two_weeks() {
        let bars = build_bars_for(&[], day("2026-09-24"));
        assert_eq!(bars.len(), 14);
        assert_eq!(bars.first().unwrap().label, "11.09");
        assert_eq!(bars.last().unwrap().label, "24.09");
    }

    #[test]
    fn days_without_activity_become_zero_bars() {
        let bars = build_bars_for(&[activity("2026-09-24", 5, 3)], day("2026-09-24"));
        let total: i64 = bars.iter().map(|bar| bar.reviews).sum();
        assert_eq!(total, 5, "чужие дни не теряются и лишние не появляются");
        assert_eq!(bars.iter().filter(|bar| bar.reviews > 0).count(), 1);
    }

    #[test]
    fn the_busiest_day_is_the_full_height_bar() {
        let bars = build_bars_for(
            &[activity("2026-09-20", 2, 1), activity("2026-09-24", 8, 6)],
            day("2026-09-24"),
        );
        let tallest = bars.iter().max_by_key(|bar| bar.reviews).unwrap();
        assert_eq!(tallest.reviews, 8);
        assert_eq!(tallest.percent, 100, "нормировка по максимуму окна");
        assert_eq!(tallest.success_percent, 75, "6 из 8");
    }

    #[test]
    fn activity_outside_the_window_is_ignored() {
        // Записи старше 14 дней не должны попадать в график: окно
        // фиксированное, иначе график рос бы бесконечно.
        let bars = build_bars_for(
            &[activity("2020-01-01", 100, 100), activity("2026-09-24", 1, 1)],
            day("2026-09-24"),
        );
        let total: i64 = bars.iter().map(|bar| bar.reviews).sum();
        assert_eq!(total, 1);
    }

    #[test]
    fn empty_history_gives_zero_bars_not_a_division_by_zero() {
        let bars = build_bars_for(&[], day("2026-09-24"));
        assert!(bars.iter().all(|bar| bar.reviews == 0 && bar.percent == 0));
    }

    #[test]
    fn successful_never_exceeds_total() {
        // Приходит из SQL; если бы счётчики разошлись, столбик успеха
        // оказался бы выше основного — график врёт.
        let bars = build_bars_for(
            &[activity("2026-09-24", 10, 4), activity("2026-09-23", 3, 3)],
            day("2026-09-24"),
        );
        for bar in &bars {
            assert!(
                bar.success_percent <= bar.percent,
                "{bar:?}: успехов больше, чем попыток"
            );
        }
    }

    #[test]
    fn percent_is_never_above_one_hundred() {
        let bars = build_bars_for(
            &[activity("2026-09-24", 1_000, 1_000), activity("2026-09-23", 1, 0)],
            day("2026-09-24"),
        );
        assert!(bars.iter().all(|bar| bar.percent <= 100));
        assert!(bars.iter().all(|bar| bar.success_percent <= 100));
    }
}

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

/// Создаёт гостевой профиль по имени и ставит cookie сессии.
/// Старый маршрут сохранён для совместимости с закладками.
pub async fn create_profile(
    State(state): State<AppState>,
    cookies: Cookies,
    Form(form): Form<NameForm>,
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

/// Завершает текущую сессию. История остаётся в базе.
pub async fn reset_profile(State(state): State<AppState>, cookies: Cookies) -> AppResult<Redirect> {
    auth::logout(&state.db, &cookies, &state.cfg.cookie_key).await?;
    Ok(Redirect::to("/"))
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


