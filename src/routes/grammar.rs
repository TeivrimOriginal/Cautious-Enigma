//! Грамматические упражнения: по одному заданию за раз, проверка через JSON API.

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;

use crate::auth::MaybeProfile;
use crate::error::AppResult;
use crate::routes::{NavContext, html, nav_context, page_impl};
use crate::{AppState, queries};

#[derive(Debug, askama::Template)]
#[template(path = "grammar.html")]
pub struct GrammarPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub id: i64,
    pub topic: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub number: i64,
    pub total: i64,
    pub next_offset: i64,
}

page_impl!(GrammarPage {
    id: i64,
    topic: String,
    prompt: String,
    options: Vec<String>,
    number: i64,
    total: i64,
    next_offset: i64,
});

#[derive(Debug, Deserialize)]
pub struct GrammarQuery {
    #[serde(default)]
    n: i64,
}

pub async fn index(
    State(state): State<AppState>,
    MaybeProfile(profile): MaybeProfile,
    Query(query): Query<GrammarQuery>,
) -> AppResult<Html<String>> {
    let nav = nav_context(&state, profile.as_ref()).await?;
    let (exercise, number, total) = queries::exercise_at(&state.db, query.n).await?;

    let mut page = GrammarPage::new(nav, "Грамматика", "grammar");
    page.id = exercise.id;
    page.topic = exercise.topic;
    page.prompt = exercise.prompt;
    page.options = exercise.options.0;
    page.number = number;
    page.total = total;
    page.next_offset = number;
    html(page)
}
