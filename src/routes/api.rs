//! JSON API: перевод слова по клику, добавление карточки и проверка грамматики.

use axum::Json;
use axum::extract::{Query, State};
use serde::{Deserialize, Serialize};

use crate::auth::{MaybeProfile, Profile};
use crate::error::{ApiResult, AppError};
use crate::{AppState, queries, seed};

#[derive(Debug, Deserialize)]
pub struct TranslateQuery {
    #[serde(default)]
    pub word: String,
}

#[derive(Debug, Serialize)]
pub struct TranslateResponse {
    pub word: String,
    pub translation: String,
    pub example: Option<String>,
    pub in_cards: bool,
    pub can_add: bool,
}

/// `GET /api/translate?word=...` — перевод слова из встроенного словаря.
pub async fn translate(
    State(state): State<AppState>,
    MaybeProfile(profile): MaybeProfile,
    Query(query): Query<TranslateQuery>,
) -> ApiResult<Json<TranslateResponse>> {
    let word = query.word.trim();
    if word.is_empty() || word.chars().count() > 64 {
        return Err(AppError::BadRequest("Некорректное слово".into()).into());
    }

    let Some(entry) = seed::lookup(word) else {
        return Err(AppError::NotFound.into());
    };

    let mut in_cards = false;
    if let Some(profile) = profile.as_ref() {
        in_cards = queries::word_in_cards(&state.db, profile.id, &entry.front).await?;
        if !in_cards {
            queries::log_translation(&state.db, profile.id, word).await?;
        }
    }

    Ok(Json(TranslateResponse {
        word: entry.front.clone(),
        translation: entry.back.clone(),
        example: entry.example.clone(),
        in_cards,
        can_add: profile.is_some() && !in_cards,
    }))
}

#[derive(Debug, Deserialize)]
pub struct AddCardRequest {
    #[serde(default)]
    pub front: String,
    #[serde(default)]
    pub back: String,
    #[serde(default)]
    pub example: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddCardResponse {
    pub added: bool,
    pub front: String,
    pub back: String,
}

/// `POST /api/cards` — добавить слово в карточки из окна перевода.
pub async fn add_card(
    State(state): State<AppState>,
    profile: Profile,
    Json(request): Json<AddCardRequest>,
) -> ApiResult<Json<AddCardResponse>> {
    let front = request.front.trim();
    let back = request.back.trim();
    if front.is_empty() || back.is_empty() {
        return Err(AppError::BadRequest("Нужны слово и перевод".into()).into());
    }

    // Если слово есть в словаре проекта, подставляем проверенный перевод.
    let (front, back) = match seed::lookup(front) {
        Some(entry) if back.is_empty() => (entry.front.clone(), entry.back.clone()),
        _ => (front.to_string(), back.to_string()),
    };

    let example = request
        .example
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| seed::lookup(&front).and_then(|entry| entry.example.clone()));

    let added =
        queries::insert_card(&state.db, profile.id, &front, &back, example.as_deref()).await?;

    Ok(Json(AddCardResponse { added, front, back }))
}

#[derive(Debug, Deserialize)]
pub struct GrammarCheckRequest {
    pub id: i64,
    pub answer: usize,
}

#[derive(Debug, Serialize)]
pub struct GrammarCheckResponse {
    pub correct: bool,
    pub correct_index: usize,
    pub correct_option: String,
    pub explanation: String,
    pub xp: i32,
}

/// `POST /api/grammar/check` — проверка ответа на упражнение.
pub async fn check_grammar(
    State(state): State<AppState>,
    MaybeProfile(profile): MaybeProfile,
    Json(request): Json<GrammarCheckRequest>,
) -> ApiResult<Json<GrammarCheckResponse>> {
    let exercise = queries::exercise_by_id(&state.db, request.id).await?;
    let correct_index = exercise.correct_index.max(0) as usize;

    let Some(correct_option) = exercise.options.0.get(correct_index).cloned() else {
        return Err(AppError::Internal("повреждённое упражнение".into()).into());
    };

    let correct = request.answer == correct_index;

    // Ответ попадает в журнал: из него считаются опыт и достижения.
    if let Some(profile) = profile.as_ref() {
        queries::log_grammar(&state.db, profile.id, exercise.id, correct).await?;
    }

    Ok(Json(GrammarCheckResponse {
        correct,
        correct_index,
        correct_option,
        explanation: exercise.explanation.unwrap_or_default(),
        xp: crate::stats::grammar_xp(correct),
    }))
}

#[derive(Debug, Deserialize)]
pub struct GoalRequest {
    pub goal: i32,
}

#[derive(Debug, Serialize)]
pub struct GoalResponse {
    pub goal: i32,
}

/// `POST /api/goal` — изменить дневную цель по количеству повторений.
pub async fn set_goal(
    State(state): State<AppState>,
    profile: Profile,
    Json(request): Json<GoalRequest>,
) -> ApiResult<Json<GoalResponse>> {
    let goal = request.goal.clamp(5, 100);
    queries::set_daily_goal(&state.db, profile.id, goal).await?;
    Ok(Json(GoalResponse { goal }))
}
