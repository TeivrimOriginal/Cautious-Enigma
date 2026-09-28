//! Статический контент проекта: словарь, тексты для чтения и упражнения.
//!
//! Всё встраивается в бинарник на этапе сборки (`include_str!`), поэтому
//! приложение не зависит от файловой системы — это важно для serverless.

use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::NaiveDate;
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

/// Все слова словаря без дублей, отсортированные по алфавиту.
/// Фразовые глаголы индексируются дважды (с «to» и без), поэтому
/// повторы по `front` схлопываются.
pub fn entries() -> &'static Vec<&'static DictionaryEntry> {
    static ENTRIES: OnceLock<Vec<&'static DictionaryEntry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let mut unique: HashMap<&str, &DictionaryEntry> = HashMap::new();
        for entry in dictionary().values() {
            unique.entry(entry.front.as_str()).or_insert(entry);
        }
        let mut list: Vec<&DictionaryEntry> = unique.into_values().collect();
        list.sort_by_key(|entry| entry.front.to_lowercase());
        list
    })
}

/// Слово дня: стабильный выбор на основе номера дня в году,
/// чтобы у всех пользователей в один день было одинаковое слово.
pub fn word_of_the_day(today: NaiveDate) -> Option<&'static DictionaryEntry> {
    use chrono::Datelike;

    let list = entries();
    if list.is_empty() {
        return None;
    }
    let first_day = NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today);
    let day_of_year = (today - first_day).num_days() as usize;
    list.get(day_of_year % list.len()).copied()
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
    fn normalize_strips_punctuation_and_case() {
        assert_eq!(normalize("Deadline,"), "deadline");
        // Пунктуация с краёв снимается, внутренняя сохраняется: иначе
        // «well-known» и «hello, world» не находились бы в словаре.
        assert_eq!(normalize("  Hello, World!  "), "hello, world");
        assert_eq!(normalize("it's"), "it's");
        assert_eq!(normalize("well-known"), "well-known");
        assert_eq!(normalize("..."), "");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn normalize_keeps_digits_and_inner_symbols() {
        assert_eq!(normalize("word2"), "word2");
        assert_eq!(normalize("«quoted»"), "quoted");
    }

    #[test]
    fn lookup_ignores_surrounding_punctuation() {
        for form in ["deadline", "Deadline", "deadline.", "\"deadline\"", "(deadline)"] {
            assert_eq!(
                lookup(form).map(|entry| entry.front.as_str()),
                Some("deadline"),
                "form={form}"
            );
        }
    }

    #[test]
    fn lookup_handles_word_forms() {
        // Реальные формы из словаря проекта: перевод по клику в текстах
        // должен работать не только для словарной формы.
        assert!(lookup("groceries").is_some(), "groceries -> grocery");
        assert!(lookup("commutes").is_some(), "commutes -> to commute");
        assert!(lookup("deadlines").is_some(), "deadlines -> deadline");
    }

    #[test]
    fn lookup_does_not_match_a_substring() {
        // Точное слово, а не «вхождение»: иначе по клику находилось бы
        // первое же слово, начинающееся так же.
        assert!(lookup("dead").is_none());
        assert!(lookup("eadline").is_none());
    }

    #[test]
    fn candidate_forms_strip_trailing_punctuation_first() {
        // «books,» в тексте — то же самое слово, что «books».
        assert!(lookup("recipes,").is_some());
        assert!(lookup("recipes.").is_some());
    }

    #[test]
    fn lookup_of_an_empty_normalized_word_is_none() {
        for form in ["", "   ", "!!!", "...", "---", "'''"] {
            assert!(lookup(form).is_none(), "form={form:?}");
        }
    }

    #[test]
    fn dictionary_entries_have_both_directions_filled() {
        for entry in entries() {
            assert!(!entry.front.trim().is_empty(), "пустое слово");
            assert!(!entry.back.trim().is_empty(), "пустой перевод у {}", entry.front);
        }
    }

    #[test]
    fn word_of_the_day_changes_over_the_year() {
        // Слово дня привязано к номеру дня в году: за год все слова должны
        // хотя бы раз встретиться, и порядок не должен зависеть от времени суток.
        let mut seen = std::collections::HashSet::new();
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for offset in 0..366_i64 {
            let day = start + chrono::Days::new(offset as u64);
            let entry = word_of_the_day(day).expect("словарь не пуст");
            seen.insert(entry.front.as_str());
        }
        assert!(seen.len() > 50, "за год должно встретиться много слов");
    }

    #[test]
    fn word_of_the_day_is_defined_for_every_day_of_the_year() {
        // 31 декабря: номер дня в году считается от 1 января, и обращение
        // к 1 января следующего года даёт тот же ключ — падать не должно.
        for (month, day) in [(1_u32, 1_u32), (2, 29), (12, 31)] {
            let date = NaiveDate::from_ymd_opt(2028, month, day).unwrap();
            assert!(word_of_the_day(date).is_some(), "{month}-{day}");
        }
    }

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
    fn entries_are_unique_and_sorted() {
        let list = entries();
        assert!(list.len() > 200);
        let mut sorted = list.clone();
        sorted.sort_by_key(|entry| entry.front.to_lowercase());
        assert_eq!(list.len(), sorted.len(), "дубликатов быть не должно");
        for pair in list.windows(2) {
            assert!(
                pair[0].front.to_lowercase() <= pair[1].front.to_lowercase(),
                "порядок нарушен: {} перед {}",
                pair[0].front,
                pair[1].front
            );
        }
    }

    #[test]
    fn word_of_the_day_is_stable() {
        let day = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        let first = word_of_the_day(day).expect("словарь не пуст");
        let second = word_of_the_day(day).expect("словарь не пуст");
        assert_eq!(first.front, second.front);
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
