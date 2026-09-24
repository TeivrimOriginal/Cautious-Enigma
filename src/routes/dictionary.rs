//! Страница словаря: поиск по всем словам и добавление в карточки.

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;

use crate::auth::MaybeProfile;
use crate::error::AppResult;
use crate::routes::{NavContext, html, nav_context, page_impl};
use crate::{AppState, queries, seed};

#[derive(Debug, askama::Template)]
#[template(path = "dictionary.html")]
pub struct DictionaryPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    /// Слово дня: (английское, перевод, пример).
    pub word_of_day: Option<(String, String, String)>,
    pub query: String,
    pub words: Vec<WordView>,
    pub total: usize,
}

page_impl!(DictionaryPage {
    word_of_day: Option<(String, String, String)>,
    query: String,
    words: Vec<WordView>,
    total: usize,
});

#[derive(Debug, Clone)]
pub struct WordView {
    pub front: String,
    pub back: String,
    pub example: String,
    pub in_cards: bool,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

pub async fn index(
    State(state): State<AppState>,
    MaybeProfile(profile): MaybeProfile,
    Query(query): Query<SearchQuery>,
) -> AppResult<Html<String>> {
    let nav = nav_context(&state, profile.as_ref()).await?;
    let needle = query.q.trim().to_lowercase();

    let all = seed::entries();
    let total = all.len();

    let words: Vec<WordView> = all
        .iter()
        .filter(|entry| {
            needle.is_empty()
                || entry.front.to_lowercase().contains(&needle)
                || entry.back.to_lowercase().contains(&needle)
        })
        .take(300)
        .map(|entry| WordView {
            front: entry.front.clone(),
            back: entry.back.clone(),
            example: entry.example.clone().unwrap_or_default(),
            in_cards: false,
        })
        .collect();

    // Отмечаем слова, которые уже есть в карточках профиля: один запрос вместо N.
    let mut words = words;
    if let Some(profile) = profile.as_ref() {
        let fronts = queries::card_fronts(&state.db, profile.id).await?;
        let lower: Vec<String> = fronts.iter().map(|front| front.to_lowercase()).collect();
        for word in words.iter_mut() {
            word.in_cards = lower.contains(&word.front.to_lowercase());
        }
    }

    let word_of_day = seed::word_of_the_day(crate::routes::today()).map(|entry| {
        (
            entry.front.clone(),
            entry.back.clone(),
            entry.example.clone().unwrap_or_default(),
        )
    });

    let mut page = DictionaryPage::new(nav, "Словарь", "dictionary");
    page.word_of_day = word_of_day;
    page.query = query.q.trim().to_string();
    page.words = words;
    page.total = total;
    html(page)
}
