//! Запросы к базе данных, собранные в одном месте.
//!
//! Роуты остаются тонкими: только разбор запроса, вызов функции и рендеринг.

use chrono::{NaiveDate, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{CardRow, DailyActivity, GrammarRow, ReviewSummary, TextRow};
use crate::sm2::Sm2State;

const SUMMARY_SQL: &str = r#"
SELECT
    (SELECT count(*) FROM cards WHERE profile_id = $1)                          AS total_cards,
    (SELECT count(*) FROM cards WHERE profile_id = $1 AND repetitions > 0)     AS learned_cards,
    (SELECT count(*) FROM cards WHERE profile_id = $1 AND repetitions = 0)     AS new_cards,
    (SELECT count(*) FROM cards
      WHERE profile_id = $1 AND due_date IS NOT NULL AND due_date <= CURRENT_DATE) AS due_today,
    (SELECT count(*) FROM review_log WHERE profile_id = $1)                    AS total_reviews,
    (SELECT count(*) FROM review_log WHERE profile_id = $1 AND quality >= 3)    AS successful_reviews,
    (SELECT count(*) FROM review_log
      WHERE profile_id = $1 AND reviewed_at >= date_trunc('day', now()))       AS reviews_today,
    (SELECT count(*) FROM translation_log WHERE profile_id = $1)               AS translations
"#;

pub async fn review_summary(pool: &PgPool, profile_id: Uuid) -> AppResult<ReviewSummary> {
    Ok(sqlx::query_as::<_, ReviewSummary>(SUMMARY_SQL)
        .bind(profile_id)
        .fetch_one(pool)
        .await?)
}

/// Активность по дням за последние `days` дней (для графика и стриков).
pub async fn daily_activity(
    pool: &PgPool,
    profile_id: Uuid,
    days: i32,
) -> AppResult<Vec<DailyActivity>> {
    Ok(sqlx::query_as::<_, DailyActivity>(
        "SELECT reviewed_at::date AS day,
                count(*)          AS reviews,
                count(*) FILTER (WHERE quality >= 3) AS successful
         FROM review_log
         WHERE profile_id = $1
           AND reviewed_at >= now() - make_interval(days => $2)
         GROUP BY 1
         ORDER BY 1",
    )
    .bind(profile_id)
    .bind(days)
    .fetch_all(pool)
    .await?)
}

pub async fn due_cards(pool: &PgPool, profile_id: Uuid, limit: i64) -> AppResult<Vec<CardRow>> {
    Ok(sqlx::query_as::<_, CardRow>(
        "SELECT id, front, back, example, repetitions, interval_days, ease,
                due_date, created_at, last_reviewed_at
         FROM cards
         WHERE profile_id = $1
           AND due_date IS NOT NULL
           AND due_date <= CURRENT_DATE
         ORDER BY due_date ASC, repetitions ASC, id ASC
         LIMIT $2",
    )
    .bind(profile_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

pub async fn all_cards(pool: &PgPool, profile_id: Uuid) -> AppResult<Vec<CardRow>> {
    Ok(sqlx::query_as::<_, CardRow>(
        "SELECT id, front, back, example, repetitions, interval_days, ease,
                due_date, created_at, last_reviewed_at
         FROM cards
         WHERE profile_id = $1
         ORDER BY created_at DESC",
    )
    .bind(profile_id)
    .fetch_all(pool)
    .await?)
}

pub async fn card_by_id(pool: &PgPool, profile_id: Uuid, id: i64) -> AppResult<CardRow> {
    sqlx::query_as::<_, CardRow>(
        "SELECT id, front, back, example, repetitions, interval_days, ease,
                due_date, created_at, last_reviewed_at
         FROM cards WHERE id = $1 AND profile_id = $2",
    )
    .bind(id)
    .bind(profile_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn count_cards(pool: &PgPool, profile_id: Uuid) -> AppResult<i64> {
    Ok(
        sqlx::query_scalar("SELECT count(*) FROM cards WHERE profile_id = $1")
            .bind(profile_id)
            .fetch_one(pool)
            .await?,
    )
}

/// Добавляет карточку. Возвращает `false`, если такое слово уже есть.
pub async fn insert_card(
    pool: &PgPool,
    profile_id: Uuid,
    front: &str,
    back: &str,
    example: Option<&str>,
) -> AppResult<bool> {
    let due: NaiveDate = Utc::now().date_naive();
    let rows = sqlx::query(
        "INSERT INTO cards (profile_id, front, back, example, due_date)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (profile_id, front) DO NOTHING",
    )
    .bind(profile_id)
    .bind(front)
    .bind(back)
    .bind(example)
    .bind(due)
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows > 0)
}

pub async fn delete_card(pool: &PgPool, profile_id: Uuid, id: i64) -> AppResult<()> {
    let rows = sqlx::query("DELETE FROM cards WHERE id = $1 AND profile_id = $2")
        .bind(id)
        .bind(profile_id)
        .execute(pool)
        .await?
        .rows_affected();

    if rows == 0 {
        return Err(AppError::NotFound);
    }
    Ok(())
}

/// Применяет SM-2 к карточке и пишет запись в журнал в одной транзакции.
pub async fn apply_review(
    pool: &PgPool,
    profile_id: Uuid,
    card_id: i64,
    quality: u8,
    today: NaiveDate,
) -> AppResult<(Sm2State, NaiveDate)> {
    let card = card_by_id(pool, profile_id, card_id).await?;

    let current = Sm2State {
        repetitions: card.repetitions.max(0) as u32,
        interval_days: card.interval_days.max(0) as u32,
        ease: card.ease,
    };
    let (next, outcome) = current.review(quality, today);

    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE cards
         SET repetitions = $1, interval_days = $2, ease = $3,
             due_date = $4, last_reviewed_at = now()
         WHERE id = $5 AND profile_id = $6",
    )
    .bind(next.repetitions as i32)
    .bind(next.interval_days as i32)
    .bind(next.ease)
    .bind(outcome.next_due)
    .bind(card_id)
    .bind(profile_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("INSERT INTO review_log (card_id, profile_id, quality) VALUES ($1, $2, $3)")
        .bind(card_id)
        .bind(profile_id)
        .bind(quality as i16)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok((next, outcome.next_due))
}

pub async fn list_texts(pool: &PgPool) -> AppResult<Vec<TextRow>> {
    Ok(sqlx::query_as::<_, TextRow>(
        "SELECT id, slug, title, level, summary, content, word_count
         FROM texts ORDER BY id",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn text_by_slug(pool: &PgPool, slug: &str) -> AppResult<TextRow> {
    sqlx::query_as::<_, TextRow>(
        "SELECT id, slug, title, level, summary, content, word_count
         FROM texts WHERE slug = $1",
    )
    .bind(slug)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

/// Выбирает упражнение по кругу: `offset` — порядковый номер от начала,
/// чтобы пользователь прошёл все упражнения темы без повторов.
pub async fn exercise_at(pool: &PgPool, offset: i64) -> AppResult<(GrammarRow, i64, i64)> {
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM grammar_exercises")
        .fetch_one(pool)
        .await?;
    if total == 0 {
        return Err(AppError::NotFound);
    }
    let index = offset.rem_euclid(total);

    let row = sqlx::query_as::<_, GrammarRow>(
        "SELECT id, topic, prompt, options, correct_index, explanation
         FROM grammar_exercises ORDER BY id OFFSET $1 LIMIT 1",
    )
    .bind(index)
    .fetch_one(pool)
    .await?;

    Ok((row, index + 1, total))
}

pub async fn exercise_by_id(pool: &PgPool, id: i64) -> AppResult<GrammarRow> {
    sqlx::query_as::<_, GrammarRow>(
        "SELECT id, topic, prompt, options, correct_index, explanation
         FROM grammar_exercises WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn log_translation(pool: &PgPool, profile_id: Uuid, word: &str) -> AppResult<()> {
    sqlx::query("INSERT INTO translation_log (profile_id, word) VALUES ($1, $2)")
        .bind(profile_id)
        .bind(word)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn word_in_cards(pool: &PgPool, profile_id: Uuid, front: &str) -> AppResult<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM cards WHERE profile_id = $1 AND lower(front) = lower($2))",
    )
    .bind(profile_id)
    .bind(front)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}
