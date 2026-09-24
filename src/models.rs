//! Строки базы данных, отображаемые в `sqlx::FromRow`.

use chrono::{DateTime, NaiveDate, Utc};
use sqlx::types::Json;
use uuid::Uuid;

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
