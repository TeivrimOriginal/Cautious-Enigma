//! Страница статистики: стрики, точность, активность и «зрелость» карточек.

use std::collections::BTreeSet;

use axum::extract::State;
use axum::response::Html;

use crate::auth::Profile;
use crate::error::AppResult;
use crate::routes::{NavContext, html, nav_context, page_impl, today};
use crate::{AppState, queries, stats};

#[derive(Debug, askama::Template)]
#[template(path = "stats.html")]
pub struct StatsPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub total_cards: i64,
    pub learned: i64,
    pub new_cards: i64,
    pub due_today: i64,
    pub total_reviews: i64,
    pub successful_reviews: i64,
    pub reviews_today: i64,
    pub translations: i64,
    pub accuracy: f64,
    pub accuracy_text: String,
    pub current_streak: u32,
    pub longest: u32,
    pub days_active: i64,
    pub reviews_per_day: f64,
    pub reviews_per_day_text: String,
    pub buckets: Vec<Bucket>,
    pub history: Vec<DayRow>,
    pub weak: Vec<WeakView>,
    pub achievements: Vec<AchievementView>,
    pub xp: i32,
    pub level: u32,
    pub level_title: String,
    pub xp_percent: u32,
    pub grammar_correct: i64,
    pub forecast: Vec<ForecastView>,
    pub forecast_text: String,
}

/// Столбик прогноза освоения.
#[derive(Debug, Clone)]
pub struct ForecastView {
    pub label: String,
    pub percent: i32,
    pub learned: i32,
}

#[derive(Debug, Clone)]
pub struct AchievementView {
    pub title: &'static str,
    pub description: &'static str,
    pub achieved: bool,
}

/// Слабое слово для шаблона: счётчики ошибок уже посчитаны в SQL.
#[derive(Debug, Clone)]
pub struct WeakView {
    pub front: String,
    pub back: String,
    pub errors: i64,
    pub attempts: i64,
    pub mastery: f64,
}

page_impl!(StatsPage {
    total_cards: i64,
    learned: i64,
    new_cards: i64,
    due_today: i64,
    total_reviews: i64,
    successful_reviews: i64,
    reviews_today: i64,
    translations: i64,
    accuracy: f64,
    accuracy_text: String,
    current_streak: u32,
    longest: u32,
    days_active: i64,
    reviews_per_day: f64,
    reviews_per_day_text: String,
    buckets: Vec<Bucket>,
    history: Vec<DayRow>,
    weak: Vec<WeakView>,
    achievements: Vec<AchievementView>,
    xp: i32,
    level: u32,
    level_title: String,
    xp_percent: u32,
    grammar_correct: i64,
    forecast: Vec<ForecastView>,
    forecast_text: String,
});

/// Сколько карточек находится в каждой стадии освоения.
#[derive(Debug, Clone)]
pub struct Bucket {
    pub label: &'static str,
    pub count: i64,
    pub percent: f64,
    pub percent_text: String,
}

#[derive(Debug, Clone)]
pub struct DayRow {
    pub day: String,
    pub reviews: i64,
    pub successful: i64,
    pub accuracy_text: String,
}

pub async fn index(State(state): State<AppState>, profile: Profile) -> AppResult<Html<String>> {
    let nav = nav_context(&state, Some(&profile)).await?;
    let summary = queries::review_summary(&state.db, profile.id).await?;
    let activity = queries::daily_activity(&state.db, profile.id, 90).await?;
    let cards = queries::all_cards(&state.db, profile.id).await?;

    let days: BTreeSet<_> = activity.iter().map(|row| row.day).collect();
    let today = today();
    let reviews_in_window: i64 = activity.iter().map(|row| row.reviews).sum();

    let mut page = StatsPage::new(nav, "Статистика", "stats");
    page.total_cards = summary.total_cards;
    page.learned = summary.learned_cards;
    page.new_cards = summary.new_cards;
    page.due_today = summary.due_today;
    page.total_reviews = summary.total_reviews;
    page.successful_reviews = summary.successful_reviews;
    page.reviews_today = summary.reviews_today;
    page.translations = summary.translations;
    page.accuracy = stats::accuracy_percent(summary.total_reviews, summary.successful_reviews);
    page.accuracy_text = format!("{:.1}", page.accuracy);
    page.current_streak = stats::current_streak(&days, today);
    page.longest = stats::longest_streak(&days);
    page.days_active = days.len() as i64;
    page.reviews_per_day = if days.is_empty() {
        0.0
    } else {
        reviews_in_window as f64 / days.len() as f64
    };
    page.reviews_per_day_text = format!("{:.1}", page.reviews_per_day);

    // Распределение карточек по стадии освоения.
    let total = cards.len().max(1) as f64;
    let mut fresh = 0_i64;
    let mut learning = 0_i64;
    let mut familiar = 0_i64;
    let mut mastered = 0_i64;
    for card in &cards {
        match card.repetitions {
            r if r <= 0 => fresh += 1,
            r if r <= 2 => learning += 1,
            r if r <= 4 => familiar += 1,
            _ => mastered += 1,
        }
    }
    page.buckets = ["Новые", "Изучаются", "Знакомые", "Освоенные"]
        .iter()
        .zip([fresh, learning, familiar, mastered])
        .map(|(label, count)| Bucket {
            label,
            count,
            percent: count as f64 / total * 100.0,
            percent_text: format!("{:.0}", count as f64 / total * 100.0),
        })
        .collect();

    let mut history: Vec<DayRow> = activity
        .iter()
        .rev()
        .take(14)
        .map(|row| {
            let accuracy = stats::accuracy_percent(row.reviews, row.successful);
            DayRow {
                day: row.day.format("%d.%m").to_string(),
                reviews: row.reviews,
                successful: row.successful,
                accuracy_text: format!("{accuracy:.0}"),
            }
        })
        .collect();
    history.reverse();
    page.history = history;

    page.weak = queries::weak_words(&state.db, profile.id, 8)
        .await?
        .into_iter()
        .map(|word| WeakView {
            front: word.front,
            back: word.back,
            errors: word.errors,
            attempts: word.attempts,
            mastery: stats::mastery_percent(word.repetitions),
        })
        .collect();

    // Опыт, уровень и достижения.
    let totals = queries::xp_totals(&state.db, profile.id).await?;
    let xp = stats::total_xp(
        totals.successful_reviews,
        totals.failed_reviews,
        totals.cards,
        totals.grammar_total,
        totals.grammar_correct,
    );
    let level = stats::level_progress(xp);

    let weak = queries::weak_words(&state.db, profile.id, 100).await?;
    let weak_fixed = weak.iter().filter(|word| word.repetitions > 0).count() as i64;

    let input = stats::AchievementInput {
        cards: summary.total_cards,
        reviews: summary.total_reviews,
        successful: summary.successful_reviews,
        current_streak: page.current_streak,
        longest_streak: page.longest,
        grammar_correct: totals.grammar_correct,
        translations: summary.translations,
        // Комбо считается на клиенте: в серверной версии недоступно.
        best_combo: 0,
        weak_fixed,
    };

    page.achievements = stats::achievements(&input)
        .into_iter()
        .map(|item| AchievementView {
            title: item.title,
            description: item.description,
            achieved: item.achieved,
        })
        .collect();
    page.xp = xp;
    page.level = level.level;
    page.level_title = level.title().to_string();
    page.xp_percent = level.percent;
    page.grammar_correct = totals.grammar_correct;

    // Прогноз: считаем на копиях карточек, база не меняется.
    let simulated: Vec<stats::SimCard> = cards
        .iter()
        .map(|row| stats::SimCard {
            repetitions: row.repetitions.max(0) as u32,
            interval_days: row.interval_days.max(0) as u32,
            ease: row.ease,
            due_date: row.due_date,
        })
        .collect();
    let points = stats::forecast(&simulated, today, 14, 4);
    let max_learned = points
        .iter()
        .map(|point| point.learned)
        .max()
        .unwrap_or(0)
        .max(1);

    page.forecast = points
        .iter()
        .map(|point| ForecastView {
            label: point.day.format("%d.%m").to_string(),
            percent: (point.learned * 100 / max_learned).min(100),
            learned: point.learned,
        })
        .collect();
    page.forecast_text = match (points.first(), points.last()) {
        (Some(first), Some(last)) => format!(
            "Через 14 дней в работе будет {} слов (сейчас {}), освоено — {}. Всего понадобится около {} повторений.",
            last.learned, first.learned, last.mastered, last.reviews
        ),
        _ => String::new(),
    };

    html(page)
}
