//! Строки базы данных, отображаемые в `sqlx::FromRow`, и валидация
//! пользовательского ввода для карточек.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::types::Json;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

/// Пределы длины полей карточки. Держатся в одном месте, потому что их
/// используют и HTML-формы (`maxlength`), и серверная проверка: клиентскую
/// проверку можно обойти, а серверную — нет.
pub const MAX_FRONT_LEN: usize = 100;
pub const MAX_BACK_LEN: usize = 200;
pub const MAX_EXAMPLE_LEN: usize = 300;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ProfileRow {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CardRow {
    pub id: i64,
    pub front: String,
    pub back: String,
    pub example: Option<String>,
    pub repetitions: i32,
    pub interval_days: i32,
    pub ease: f64,
    pub due_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub last_reviewed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TextRow {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub level: String,
    pub summary: Option<String>,
    pub content: String,
    pub word_count: i32,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GrammarRow {
    pub id: i64,
    pub topic: String,
    pub prompt: String,
    pub options: Json<Vec<String>>,
    pub correct_index: i16,
    pub explanation: Option<String>,
}

/// Строка агрегата для страницы статистики.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReviewSummary {
    pub total_reviews: i64,
    pub successful_reviews: i64,
    pub reviews_today: i64,
    pub total_cards: i64,
    pub learned_cards: i64,
    pub due_today: i64,
    pub new_cards: i64,
    pub translations: i64,
}

/// Свод активности по дням (для графика и расчёта стрика).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DailyActivity {
    pub day: NaiveDate,
    pub reviews: i64,
    pub successful: i64,
}

/// Слово, которое пользователь забывал чаще всего.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WeakWord {
    pub id: i64,
    pub front: String,
    pub back: String,
    pub repetitions: i32,
    pub errors: i64,
    pub attempts: i64,
}

/// Проверенные и обрезанные данные карточки, готовые к записи в базу.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardInput {
    pub front: String,
    pub back: String,
    pub example: Option<String>,
}

/// Проверяет содержимое карточки перед записью.
///
/// Возвращает обрезанные от пробелов строки, поэтому и `front`, и `back`
/// гарантированно непустые — ограничение `CHECK (length(btrim(...)) > 0)`
/// в миграции не сможет сработать на данных из приложения.
pub fn validate_card(front: &str, back: &str, example: Option<&str>) -> AppResult<CardInput> {
    let front = front.trim();
    let back = back.trim();
    let example = example
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if front.is_empty() || back.is_empty() {
        return Err(AppError::BadRequest(
            "Заполните слово и перевод".to_string(),
        ));
    }
    if front.chars().count() > MAX_FRONT_LEN {
        return Err(AppError::BadRequest(format!(
            "Слово длиннее {MAX_FRONT_LEN} символов"
        )));
    }
    if back.chars().count() > MAX_BACK_LEN {
        return Err(AppError::BadRequest(format!(
            "Перевод длиннее {MAX_BACK_LEN} символов"
        )));
    }
    if let Some(example) = example.as_ref()
        && example.chars().count() > MAX_EXAMPLE_LEN
    {
        return Err(AppError::BadRequest(format!(
            "Пример длиннее {MAX_EXAMPLE_LEN} символов"
        )));
    }

    Ok(CardInput {
        front: front.to_string(),
        back: back.to_string(),
        example,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(input: Result<CardInput, AppError>) -> String {
        match input {
            Ok(_) => panic!("ожидалась ошибка, а карточка прошла проверку"),
            Err(AppError::BadRequest(message)) => message,
            Err(other) => panic!("ожидалась BadRequest, получено {other:?}"),
        }
    }

    /* ---------------------------------------------------------------- *
     * Нормализация
     * ---------------------------------------------------------------- */

    #[test]
    fn surrounding_spaces_are_trimmed() {
        let input = validate_card("  deadline  ", "  срок сдачи\t", None).unwrap();
        assert_eq!(input.front, "deadline");
        assert_eq!(input.back, "срок сдачи");
        assert_eq!(input.example, None);
    }

    #[test]
    fn example_is_trimmed_and_kept() {
        let input = validate_card("deadline", "срок", Some("  The deadline is Friday.  ")).unwrap();
        assert_eq!(input.example.as_deref(), Some("The deadline is Friday."));
    }

    #[test]
    fn blank_example_becomes_none() {
        for blank in ["", "   ", "\t\n"] {
            let input = validate_card("deadline", "срок", Some(blank)).unwrap();
            assert_eq!(input.example, None, "blank={blank:?}");
        }
    }

    #[test]
    fn internal_spaces_are_preserved() {
        let input = validate_card("to give up", "бросить  всё", None).unwrap();
        assert_eq!(input.front, "to give up");
        assert_eq!(input.back, "бросить  всё");
    }

    /* ---------------------------------------------------------------- *
     * Пустые значения
     * ---------------------------------------------------------------- */

    #[test]
    fn empty_word_is_rejected() {
        let message = error(validate_card("", "срок", None));
        assert!(message.contains("Заполните"), "сообщение: {message}");
    }

    #[test]
    fn empty_translation_is_rejected() {
        let message = error(validate_card("deadline", "", None));
        assert!(message.contains("Заполните"), "сообщение: {message}");
    }

    #[test]
    fn whitespace_only_is_rejected() {
        assert!(validate_card("   ", "срок", None).is_err());
        assert!(validate_card("deadline", " \t ", None).is_err());
    }

    /* ---------------------------------------------------------------- *
     * Границы длины
     * ---------------------------------------------------------------- */

    #[test]
    fn boundary_lengths_are_accepted() {
        let front = "a".repeat(MAX_FRONT_LEN);
        let back = "b".repeat(MAX_BACK_LEN);
        let example = "c".repeat(MAX_EXAMPLE_LEN);
        let input = validate_card(&front, &back, Some(&example)).unwrap();
        assert_eq!(input.front.chars().count(), MAX_FRONT_LEN);
        assert_eq!(input.back.chars().count(), MAX_BACK_LEN);
        assert_eq!(input.example.unwrap().chars().count(), MAX_EXAMPLE_LEN);
    }

    #[test]
    fn one_character_over_the_limit_is_rejected() {
        let front = "a".repeat(MAX_FRONT_LEN + 1);
        let back = "b".repeat(MAX_BACK_LEN + 1);
        let example = "c".repeat(MAX_EXAMPLE_LEN + 1);

        assert!(error(validate_card(&front, "срок", None)).contains("Слово"));
        assert!(error(validate_card("deadline", &back, None)).contains("Перевод"));
        assert!(error(validate_card("deadline", "срок", Some(&example))).contains("Пример"));
    }

    #[test]
    fn length_is_counted_in_characters_not_bytes() {
        // Кириллица — по 2 байта, но считается одним символом.
        let back = "щ".repeat(MAX_BACK_LEN);
        assert_eq!(back.len(), MAX_BACK_LEN * 2);
        assert!(validate_card("deadline", &back, None).is_ok());

        let too_long = "щ".repeat(MAX_BACK_LEN + 1);
        assert!(validate_card("deadline", &too_long, None).is_err());
    }

    /* ---------------------------------------------------------------- *
     * Спецсимволы
     * ---------------------------------------------------------------- */

    #[test]
    fn html_and_scripts_are_accepted_as_plain_text() {
        // Экранирование делает шаблон askama, а не валидация: отклонять
        // такой ввод здесь означало бы ломать legitimate переводы.
        let input = validate_card("<b>word</b>", "<script>alert(1)</script>", None).unwrap();
        assert_eq!(input.front, "<b>word</b>");
        assert_eq!(input.back, "<script>alert(1)</script>");
    }

    #[test]
    fn sql_metacharacters_are_kept_intact() {
        let input = validate_card("'; DROP TABLE cards; --", "1' OR '1'='1", None).unwrap();
        assert_eq!(input.front, "'; DROP TABLE cards; --");
        assert_eq!(input.back, "1' OR '1'='1");
    }

    #[test]
    fn emoji_and_long_words_are_accepted() {
        let input = validate_card("🙂", "смайл", Some("🙂🙂")).unwrap();
        assert_eq!(input.front, "🙂");
        assert_eq!(input.example.as_deref(), Some("🙂🙂"));
    }
}
