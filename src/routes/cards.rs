//! Карточки слов: список, добавление и повторение по алгоритму SM-2.

use axum::Form;
use axum::extract::{Path, Query, State};
use axum::response::{Html, Redirect};
use chrono::NaiveDate;
use serde::Deserialize;

use crate::auth::Profile;
use crate::error::{AppError, AppResult};
use crate::models::CardRow;
use crate::routes::{NavContext, html, nav_context, page_impl, today, urlencode};
use crate::{AppState, models, queries, queue, stats};

#[derive(Debug, askama::Template)]
#[template(path = "cards.html")]
pub struct CardsPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub due: Vec<CardView>,
    pub all: Vec<CardView>,
    pub total: i64,
    pub learned: i64,
    pub new_cards: i64,
    pub error: String,
    pub notice: String,
    pub weak_mode: bool,
}

page_impl!(CardsPage {
    due: Vec<CardView>,
    all: Vec<CardView>,
    total: i64,
    learned: i64,
    new_cards: i64,
    error: String,
    notice: String,
    weak_mode: bool,
});

/// Представление карточки для шаблона.
#[derive(Debug, Clone)]
pub struct CardView {
    pub id: i64,
    pub front: String,
    pub back: String,
    pub example: String,
    pub repetitions: i32,
    pub ease: f64,
    pub interval_text: String,
    pub due_text: String,
    pub mastery: f64,
    pub mastery_text: String,
    pub is_due: bool,
}

impl CardView {
    /// Представление строки базы для шаблона.
    ///
    /// Публичная, потому что правила «просрочена / сегодня / не назначена»
    /// и признак `is_due` — продуктовое поведение, а не деталь рендера:
    /// их должен проверять `queue`, а не только шаблон.
    pub fn new(row: &CardRow, today: NaiveDate) -> Self {
        // Признаки «должна повторяться» и «просрочена» берём из `queue`,
        // иначе страница и очередь разойдутся по смыслу.
        let due_text = match row.due_date {
            Some(due) if queue::is_overdue(Some(due), today) => "просрочена".to_string(),
            Some(due) if due == today => "сегодня".to_string(),
            Some(due) => due.format("%d.%m.%Y").to_string(),
            None => "не назначена".to_string(),
        };

        let mastery = stats::mastery_percent(row.repetitions);

        Self {
            id: row.id,
            front: row.front.clone(),
            back: row.back.clone(),
            example: row.example.clone().unwrap_or_default(),
            repetitions: row.repetitions,
            ease: row.ease,
            interval_text: stats::format_interval(row.interval_days),
            due_text,
            mastery,
            mastery_text: format!("{mastery:.0}"),
            is_due: queue::is_due(row.due_date, today),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Feedback {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    added: Option<String>,
    /// `1` — повторять только слабые слова, вне очереди по дате.
    #[serde(default)]
    weak: Option<String>,
    #[serde(default)]
    edited: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewCardForm {
    #[serde(default)]
    front: String,
    #[serde(default)]
    back: String,
    #[serde(default)]
    example: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewForm {
    #[serde(default)]
    quality: u8,
}

#[derive(Debug, Deserialize)]
pub struct EditCardForm {
    #[serde(default)]
    front: String,
    #[serde(default)]
    back: String,
    #[serde(default)]
    example: Option<String>,
}

pub async fn index(
    State(state): State<AppState>,
    profile: Profile,
    Query(feedback): Query<Feedback>,
) -> AppResult<Html<String>> {
    let nav = nav_context(&state, Some(&profile)).await?;
    let today = today();
    let weak_mode = feedback.weak.as_deref() == Some("1");

    let rows = queries::all_cards(&state.db, profile.id).await?;
    let mut due_rows = queries::due_cards(&state.db, profile.id, 50).await?;

    if weak_mode {
        // Слабые слова повторяем вне очереди: берём их из всех карточек
        // и сортируем по числу ошибок (по убыванию).
        let weak = queries::weak_words(&state.db, profile.id, 50).await?;
        let by_id: std::collections::HashMap<i64, (i64, i64)> = weak
            .iter()
            .map(|word| (word.id, (word.errors, word.attempts)))
            .collect();
        let mut order: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
        for (position, word) in weak.iter().enumerate() {
            order.insert(word.id, position as i64);
        }
        due_rows = rows
            .iter()
            .filter(|row| by_id.contains_key(&row.id))
            .cloned()
            .collect::<Vec<_>>();
        due_rows.sort_by_key(|row| order.get(&row.id).copied().unwrap_or(i64::MAX));
    }

    let summary = queries::review_summary(&state.db, profile.id).await?;

    let mut page = CardsPage::new(nav, "Карточки", "cards");
    page.due = due_rows
        .iter()
        .map(|row| CardView::new(row, today))
        .collect();
    page.all = rows.iter().map(|row| CardView::new(row, today)).collect();
    page.total = summary.total_cards;
    page.learned = summary.learned_cards;
    page.new_cards = summary.new_cards;
    page.error = feedback.error.unwrap_or_default();
    page.notice = match (feedback.added.as_deref(), feedback.edited.as_deref()) {
        (Some("1"), _) => "Карточка добавлена".to_string(),
        (Some("exists"), _) => "Такое слово уже есть в карточках".to_string(),
        (_, Some("1")) => "Карточка обновлена".to_string(),
        _ => String::new(),
    };
    page.weak_mode = weak_mode;
    html(page)
}

pub async fn create(
    State(state): State<AppState>,
    profile: Profile,
    Form(form): Form<NewCardForm>,
) -> AppResult<Redirect> {
    // Проверка и обрезка — в одном месте с JSON API, иначе формы и API
    // рано или поздно начнут принимать разные данные.
    let input = match models::validate_card(&form.front, &form.back, form.example.as_deref()) {
        Ok(input) => input,
        Err(AppError::BadRequest(message)) => {
            return Ok(Redirect::to(&format!(
                "/cards?error={}",
                urlencode(&message)
            )));
        }
        Err(err) => return Err(err),
    };

    // `insert_checked_card`: вход уже проверен выше, повторно валидировать
    // те же строки незачем.
    let inserted = queries::insert_checked_card(&state.db, profile.id, &input).await?;
    let flag = if inserted { "1" } else { "exists" };
    Ok(Redirect::to(&format!("/cards?added={flag}")))
}

/// Правка карточки: слово, перевод, пример.
///
/// Отдельный маршрут вместо переиспользования `create`, чтобы состояние
/// SM-2 и дата повторения не потерялись при правке текста.
pub async fn update(
    State(state): State<AppState>,
    profile: Profile,
    Path(id): Path<i64>,
    Form(form): Form<EditCardForm>,
) -> AppResult<Redirect> {
    let input = match models::validate_card(&form.front, &form.back, form.example.as_deref()) {
        Ok(input) => input,
        Err(AppError::BadRequest(message)) => {
            return Ok(Redirect::to(&format!("/cards?error={}", urlencode(&message))));
        }
        Err(err) => return Err(err),
    };

    if !queries::update_card(
        &state.db,
        profile.id,
        id,
        &input.front,
        &input.back,
        input.example.as_deref(),
    )
    .await?
    {
        return Ok(Redirect::to("/cards?error=Карточка+не+найдена"));
    }

    Ok(Redirect::to("/cards?edited=1"))
}

/// Применяет оценку повторения и возвращает к списку карточек.
pub async fn review(
    State(state): State<AppState>,
    profile: Profile,
    Path(id): Path<i64>,
    Form(form): Form<ReviewForm>,
) -> AppResult<Redirect> {
    let quality = form.quality.clamp(1, 5);
    queries::apply_review(&state.db, profile.id, id, quality, today()).await?;
    Ok(Redirect::to("/cards?reviewed=1"))
}

pub async fn delete(
    State(state): State<AppState>,
    profile: Profile,
    Path(id): Path<i64>,
) -> Result<Redirect, AppError> {
    queries::delete_card(&state.db, profile.id, id).await?;
    Ok(Redirect::to("/cards"))
}
