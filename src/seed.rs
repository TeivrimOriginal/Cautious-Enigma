//! Статический контент проекта: словарь, тексты для чтения и упражнения.
//!
//! Всё встраивается в бинарник на этапе сборки (`include_str!`), поэтому
//! приложение не зависит от файловой системы — это важно для serverless.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;
use sqlx::PgPool;

use crate::error::AppResult;

const DICTIONARY_TSV: &str = include_str!("../data/dictionary.tsv");
const TEXTS_JSON: &str = include_str!("../data/texts.json");
const GRAMMAR_JSON: &str = include_str!("../data/grammar.json");

#[derive(Debug, Clone)]
pub struct DictionaryEntry {
    pub front: String,
    pub back: String,
    pub example: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedText {
    pub slug: String,
    pub title: String,
    pub level: String,
    pub summary: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeedExercise {
    pub topic: String,
    pub prompt: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
}

impl SeedText {
    /// Количество слов в тексте — показывается в списке текстов.
    pub fn word_count(&self) -> i32 {
        self.content.split_whitespace().count() as i32
    }

    /// Абзацы текста: пустая строка разделяет абзацы.
    pub fn paragraphs(&self) -> Vec<&str> {
        self.content
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect()
    }
}

/// Разобранный словарь: ключ — нормализованное слово в нижнем регистре.
fn dictionary() -> &'static HashMap<String, DictionaryEntry> {
    static DICT: OnceLock<HashMap<String, DictionaryEntry>> = OnceLock::new();
    DICT.get_or_init(|| {
        let mut map = HashMap::new();
        for line in DICTIONARY_TSV.lines() {
            let line = line.trim_end_matches('\r');
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let Some(front) = fields.next() else { continue };
            let Some(back) = fields.next() else { continue };
            let example = fields.next().map(str::trim).filter(|s| !s.is_empty());

            let front = front.trim();
            if front.is_empty() || back.trim().is_empty() {
                continue;
            }
            let entry = DictionaryEntry {
                front: front.to_string(),
                back: back.trim().to_string(),
                example: example.map(str::to_string),
            };
            map.insert(normalize(front), entry.clone());

            // Фразовые глаголы хранятся как «to boil», но в тексте встречаются
            // формы без «to»: дополнительно индексируем основу.
            if let Some(stem) = front.strip_prefix("to ") {
                map.insert(normalize(stem), entry);
            }
        }
        map
    })
}

/// Ищет слово в словаре. Если точной формы нет, пробует простые
/// морфологические варианты (`books` → `book`, `running` → `run`).
pub fn lookup(word: &str) -> Option<&'static DictionaryEntry> {
    let key = normalize(word);
    if key.is_empty() {
        return None;
    }
    let dict = dictionary();
    if let Some(entry) = dict.get(&key) {
        return Some(entry);
    }
    candidate_forms(&key)
        .into_iter()
        .find_map(|form| dict.get(&form))
}

/// Приводит слово к ключу словаря: нижний регистр, без пунктуации по краям.
pub fn normalize(word: &str) -> String {
    word.trim()
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '\'' && c != '-')
        .to_lowercase()
}

/// Простые словоформы, которые пробуем при промахе.
fn candidate_forms(word: &str) -> Vec<String> {
    let mut forms = Vec::new();
    let w = word.trim_end_matches(['.', ',', '!', '?']);

    if let Some(stem) = w.strip_suffix("ies") {
        forms.push(format!("{stem}y"));
    }
    for suffix in ["ing", "ed", "es", "s"] {
        if let Some(stem) = w.strip_suffix(suffix) {
            if suffix == "es" && w.ends_with("ses") {
                forms.push(stem.to_string());
            }
            forms.push(stem.to_string());
            forms.push(format!("{stem}e"));
            // удвоенная согласная: running -> run, stopped -> stop
            let mut chars = stem.chars();
            if let (Some(last), Some(prev)) = (chars.next_back(), chars.clone().next_back())
                && last == prev
                && !"aeiou".contains(last)
            {
                let head: String = chars.collect();
                forms.push(head);
            }
        }
    }
    if let Some(stem) = w.strip_suffix("ly") {
        forms.push(stem.to_string());
    }

    forms.retain(|f| !f.is_empty() && f != w);
    forms.sort();
    forms.dedup();
    forms
}

pub fn texts() -> &'static [SeedText] {
    static TEXTS: OnceLock<Vec<SeedText>> = OnceLock::new();
    TEXTS.get_or_init(|| {
        serde_json::from_str(TEXTS_JSON)
            .expect("data/texts.json повреждён или имеет неверный формат")
    })
}

pub fn exercises() -> &'static [SeedExercise] {
    static EXERCISES: OnceLock<Vec<SeedExercise>> = OnceLock::new();
    EXERCISES.get_or_init(|| {
        serde_json::from_str(GRAMMAR_JSON)
            .expect("data/grammar.json повреждён или имеет неверный формат")
    })
}

/// Наполняет справочные таблицы, если они пусты. Идемпотентно:
/// повторный холодный старт на Vercel не дублирует данные.
pub async fn run(pool: &PgPool) -> AppResult<()> {
    seed_texts(pool).await?;
    seed_grammar(pool).await?;
    Ok(())
}

async fn seed_texts(pool: &PgPool) -> AppResult<()> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM texts")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    for text in texts() {
        sqlx::query(
            "INSERT INTO texts (slug, title, level, summary, content, word_count)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (slug) DO NOTHING",
        )
        .bind(&text.slug)
        .bind(&text.title)
        .bind(&text.level)
        .bind(&text.summary)
        .bind(&text.content)
        .bind(text.word_count())
        .execute(pool)
        .await?;
    }
    tracing::info!(count = texts().len(), "тексты для чтения загружены");
    Ok(())
}

async fn seed_grammar(pool: &PgPool) -> AppResult<()> {
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM grammar_exercises")
        .fetch_one(pool)
        .await?;
    if count > 0 {
        return Ok(());
    }

    for exercise in exercises() {
        sqlx::query(
            "INSERT INTO grammar_exercises (topic, prompt, options, correct_index, explanation)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&exercise.topic)
        .bind(&exercise.prompt)
        .bind(&exercise.options)
        .bind(exercise.correct_index as i16)
        .bind(&exercise.explanation)
        .execute(pool)
        .await?;
    }
    tracing::info!(
        count = exercises().len(),
        "грамматические упражнения загружены"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_is_parsed() {
        let dict = dictionary();
        assert!(
            dict.len() > 200,
            "ожидалось более 200 слов, got {}",
            dict.len()
        );
        let entry = dict
            .get("deadline")
            .expect("слово deadline должно быть в словаре");
        assert_eq!(entry.back, "срок сдачи");
    }

    #[test]
    fn lookup_normalizes_case_and_punctuation() {
        let entry = lookup("Deadline,").expect("поиск без учёта регистра и запятой");
        assert_eq!(entry.front, "deadline");
    }

    #[test]
    fn lookup_finds_inflected_forms() {
        assert!(lookup("deadlines").is_some(), "deadlines -> deadline");
        assert!(lookup("recipes").is_some(), "recipes -> recipe");
        assert!(lookup("boiling").is_some(), "boiling -> to boil");
        assert!(lookup("boiled").is_some(), "boiled -> to boil");
    }

    #[test]
    fn lookup_returns_none_for_unknown() {
        assert!(lookup("zzzznotaword").is_none());
        assert!(lookup("   ").is_none());
    }

    #[test]
    fn text_paragraphs_split_correctly() {
        let text = SeedText {
            slug: "t".into(),
            title: "t".into(),
            level: "A2".into(),
            summary: "s".into(),
            content: "First paragraph.\n\nSecond paragraph.".into(),
        };
        assert_eq!(text.paragraphs().len(), 2);
        assert_eq!(text.word_count(), 4);
    }

    #[test]
    fn seed_data_is_valid() {
        assert!(texts().len() >= 3, "нужно минимум 3 текста");
        assert!(exercises().len() >= 15, "нужно минимум 15 упражнений");
        for exercise in exercises() {
            assert!(
                exercise.correct_index < exercise.options.len(),
                "неверный correct_index в упражнении: {}",
                exercise.prompt
            );
            assert!(exercise.options.len() >= 2);
        }
    }
}
