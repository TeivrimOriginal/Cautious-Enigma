//! Чтение текстов: список и страница текста с переводом слов по клику.

use axum::extract::{Path, State};
use axum::response::Html;

use crate::auth::MaybeProfile;
use crate::error::AppResult;
use crate::routes::{NavContext, html, nav_context, page_impl};
use crate::{AppState, queries};

#[derive(Debug, askama::Template)]
#[template(path = "reading_list.html")]
pub struct ReadingList {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub texts: Vec<TextCard>,
}

page_impl!(ReadingList { texts: Vec<TextCard> });

#[derive(Debug, askama::Template)]
#[template(path = "reading_text.html")]
pub struct ReadingText {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    pub title_text: String,
    pub level: String,
    pub summary: String,
    pub paragraphs: Vec<String>,
    pub word_count: i32,
    pub other: Vec<TextCard>,
}

page_impl!(ReadingText {
    title_text: String,
    level: String,
    summary: String,
    paragraphs: Vec<String>,
    word_count: i32,
    other: Vec<TextCard>,
});

#[derive(Debug, Clone)]
pub struct TextCard {
    pub slug: String,
    pub title: String,
    pub level: String,
    pub summary: String,
    pub word_count: i32,
}

pub async fn index(
    State(state): State<AppState>,
    MaybeProfile(profile): MaybeProfile,
) -> AppResult<Html<String>> {
    let nav = nav_context(&state, profile.as_ref()).await?;
    let rows = queries::list_texts(&state.db).await?;

    let mut page = ReadingList::new(nav, "Тексты для чтения", "reading");
    page.texts = rows
        .iter()
        .map(|row| TextCard {
            slug: row.slug.clone(),
            title: row.title.clone(),
            level: row.level.clone(),
            summary: row.summary.clone().unwrap_or_default(),
            word_count: row.word_count,
        })
        .collect();
    html(page)
}

pub async fn show(
    State(state): State<AppState>,
    MaybeProfile(profile): MaybeProfile,
    Path(slug): Path<String>,
) -> AppResult<Html<String>> {
    let nav = nav_context(&state, profile.as_ref()).await?;
    let row = queries::text_by_slug(&state.db, &slug).await?;
    let all = queries::list_texts(&state.db).await?;

    let mut page = ReadingText::new(nav, row.title.clone(), "reading");
    page.title_text = row.title.clone();
    page.level = row.level.clone();
    page.summary = row.summary.clone().unwrap_or_default();
    page.paragraphs = row
        .content
        .split("\n\n")
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    page.word_count = row.word_count;
    page.other = all
        .iter()
        .filter(|other| other.slug != row.slug)
        .map(|other| TextCard {
            slug: other.slug.clone(),
            title: other.title.clone(),
            level: other.level.clone(),
            summary: other.summary.clone().unwrap_or_default(),
            word_count: other.word_count,
        })
        .collect();
    page.other.truncate(4);

    html(page)
}
