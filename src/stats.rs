//! Расчёты статистики. Вынесены в отдельный модуль без зависимостей от БД,
//! поэтому логику стриков и точности можно покрыть обычными юнит-тестами.

use std::collections::BTreeSet;

use chrono::NaiveDate;

/// Текущий стрик: дни подряд с хотя бы одним повторением.
///
/// Стрик не прерывается, если сегодня повторений ещё не было, но вчера были:
/// иначе стрик обнулялся бы каждое утро до первого повторения.
pub fn current_streak(days: &BTreeSet<NaiveDate>, today: NaiveDate) -> u32 {
    let yesterday = today.pred_opt().expect("дата вне диапазона chrono");
    let Some(start) = days
        .get(&today)
        .map(|_| today)
        .or_else(|| days.get(&yesterday).map(|_| yesterday))
    else {
        return 0;
    };

    let mut streak = 0_u32;
    let mut cursor = start;
    while days.contains(&cursor) {
        streak += 1;
        match cursor.pred_opt() {
            Some(prev) => cursor = prev,
            None => break,
        }
    }
    streak
}

/// Самая длинная серия дней подряд за всю историю.
pub fn longest_streak(days: &BTreeSet<NaiveDate>) -> u32 {
    let mut best = 0_u32;
    let mut current = 0_u32;
    let mut previous: Option<NaiveDate> = None;

    for day in days {
        current = match previous {
            Some(prev) if prev.succ_opt() == Some(*day) => current + 1,
            _ => 1,
        };
        best = best.max(current);
        previous = Some(*day);
    }
    best
}

/// Доля успешных ответов (quality >= 3) в процентах.
pub fn accuracy_percent(total: i64, successful: i64) -> f64 {
    if total <= 0 {
        return 0.0;
    }
    (successful as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
}

/// Насколько хорошо карточка освоена: 0 (новая) … 100 (зрелая).
///
/// Считается по числу успешных повторений: 1 — 20%, 2 — 40%, 3 — 60%,
/// 4 — 80%, 5+ — 100%. Удержание интервала показывается отдельно.
pub fn mastery_percent(repetitions: i32) -> f64 {
    match repetitions {
        r if r <= 0 => 0.0,
        r => ((r.min(5) as f64) * 20.0).clamp(0.0, 100.0),
    }
}

/// Человекочитаемый интервал: «через 1 день», «через 6 дней», «через 3 месяца».
pub fn format_interval(days: i32) -> String {
    match days {
        d if d <= 0 => "сейчас".to_string(),
        1 => "через 1 день".to_string(),
        2..=4 => format!("через {days} дня"),
        5..=20 => format!("через {days} дней"),
        21..=60 => format!("через {} мес.", days / 30),
        61..=365 => format!("через {} мес.", days / 30),
        _ => format!("через {:.1} года", days as f64 / 365.0),
    }
}

/* ------------------------------------------------------------------ *
 * Опыт, уровни и достижения
 * ------------------------------------------------------------------ */

/// Награды за действия. Единый набор для серверной и статической версий.
pub const XP_REVIEW_SUCCESS: i32 = 10;
pub const XP_REVIEW_FAIL: i32 = 2;
pub const XP_NEW_CARD: i32 = 5;
pub const XP_GRAMMAR_CORRECT: i32 = 15;
pub const XP_GRAMMAR_WRONG: i32 = 3;

/// Сколько опыта нужно для первого уровня; каждая следующая дороже на 50.
const XP_FIRST_LEVEL: i32 = 100;
const XP_LEVEL_STEP: i32 = 50;

/// Прогресс по уровню: сколько всего, сколько в текущем, сколько нужно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelProgress {
    pub level: u32,
    pub in_level: i32,
    pub needed: i32,
    pub percent: u32,
}

impl LevelProgress {
    /// Название уровня — даёт ощущение прогресса.
    pub fn title(&self) -> &'static str {
        match self.level {
            0 | 1 => "Новичок",
            2 => "Ученик",
            3..=4 => "Практик",
            5..=6 => "Знаток",
            7..=9 => "Продвинутый",
            _ => "Мастер",
        }
    }
}

/// Считает уровень по накопленному опыту.
pub fn level_progress(xp: i32) -> LevelProgress {
    let xp = xp.max(0);
    let mut level = 1_u32;
    let mut needed = XP_FIRST_LEVEL;
    let mut spent = 0_i32;

    while xp - spent >= needed {
        spent += needed;
        level += 1;
        needed += XP_LEVEL_STEP;
    }

    let in_level = xp - spent;
    let percent = ((in_level as f64 / needed as f64) * 100.0).round() as u32;

    LevelProgress {
        level,
        in_level,
        needed,
        percent: percent.min(100),
    }
}

/// Входные данные для проверки достижений.
#[derive(Debug, Clone, Copy, Default)]
pub struct AchievementInput {
    pub cards: i64,
    pub reviews: i64,
    pub successful: i64,
    pub current_streak: u32,
    pub longest_streak: u32,
    pub grammar_correct: i64,
    pub translations: i64,
    pub best_combo: u32,
    pub weak_fixed: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Achievement {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub achieved: bool,
}

/// Список достижений с признаком получения. Порядок — от простых к сложным.
pub fn achievements(input: &AchievementInput) -> Vec<Achievement> {
    let accuracy = accuracy_percent(input.reviews, input.successful);
    let list: [(&'static str, &'static str, &'static str, bool); 12] = [
        (
            "first-word",
            "Первые шаги",
            "Добавить первую карточку",
            input.cards >= 1,
        ),
        (
            "ten-words",
            "Коллекционер",
            "Собрать 10 карточек",
            input.cards >= 10,
        ),
        (
            "hundred-words",
            "Большой словарь",
            "Собрать 100 карточек",
            input.cards >= 100,
        ),
        (
            "first-review",
            "Первое повторение",
            "Ответить на первую карточку",
            input.reviews >= 1,
        ),
        (
            "fifty-reviews",
            "Полсотни повторений",
            "Сделать 50 повторений",
            input.reviews >= 50,
        ),
        (
            "two-hundred-reviews",
            "Двести повторений",
            "Сделать 200 повторений",
            input.reviews >= 200,
        ),
        (
            "week-streak",
            "Неделя подряд",
            "Заниматься 7 дней без перерыва",
            input.current_streak >= 7 || input.longest_streak >= 7,
        ),
        (
            "month-streak",
            "Месяц дисциплины",
            "Заниматься 30 дней без перерыва",
            input.current_streak >= 30 || input.longest_streak >= 30,
        ),
        (
            "sharp-mind",
            "Точность 90%",
            "Держать точность выше 90% (от 20 ответов)",
            input.reviews >= 20 && accuracy >= 90.0,
        ),
        (
            "combo-10",
            "Серия из десяти",
            "10 правильных ответов подряд",
            input.best_combo >= 10,
        ),
        (
            "grammar-20",
            "Грамматика",
            "20 правильных ответов в упражнениях",
            input.grammar_correct >= 20,
        ),
        (
            "weak-fixed",
            "Из сложного в простое",
            "Тренировать 5 слабых слов и ответить на них верно",
            input.weak_fixed >= 5,
        ),
    ];

    list.iter()
        .map(|(id, title, description, achieved)| Achievement {
            id,
            title,
            description,
            achieved: *achieved,
        })
        .collect()
}

/// Сколько опыта даёт ответ на карточку.
pub fn review_xp(quality: u8) -> i32 {
    if quality >= 3 {
        XP_REVIEW_SUCCESS
    } else {
        XP_REVIEW_FAIL
    }
}

/// Сколько опыта даёт ответ на упражнение.
pub fn grammar_xp(correct: bool) -> i32 {
    if correct {
        XP_GRAMMAR_CORRECT
    } else {
        XP_GRAMMAR_WRONG
    }
}

/// Итоговый опыт по накопленным счётчикам.
///
/// Считается из того, что уже есть в базе, поэтому переживает
/// пересчёт и не зависит от того, где был поставлен галочки.
pub fn total_xp(
    successful_reviews: i64,
    failed_reviews: i64,
    cards: i64,
    grammar_total: i64,
    grammar_correct: i64,
) -> i32 {
    let grammar_wrong = (grammar_total - grammar_correct).max(0);
    (successful_reviews * i64::from(XP_REVIEW_SUCCESS)
        + failed_reviews * i64::from(XP_REVIEW_FAIL)
        + cards * i64::from(XP_NEW_CARD)
        + grammar_correct * i64::from(XP_GRAMMAR_CORRECT)
        + grammar_wrong * i64::from(XP_GRAMMAR_WRONG)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn date(ymd: &str) -> NaiveDate {
        NaiveDate::parse_from_str(ymd, "%Y-%m-%d").unwrap()
    }

    fn set(values: &[&str]) -> BTreeSet<NaiveDate> {
        values.iter().map(|v| date(v)).collect()
    }

    #[test]
    fn streak_counts_consecutive_days_ending_today() {
        let days = set(&["2026-09-22", "2026-09-23", "2026-09-24"]);
        assert_eq!(current_streak(&days, date("2026-09-24")), 3);
    }

    #[test]
    fn streak_survives_when_today_is_empty() {
        let days = set(&["2026-09-22", "2026-09-23"]);
        assert_eq!(current_streak(&days, date("2026-09-24")), 2);
    }

    #[test]
    fn streak_is_zero_after_gap() {
        let days = set(&["2026-09-20", "2026-09-24"]);
        assert_eq!(current_streak(&days, date("2026-09-24")), 1);
    }

    #[test]
    fn streak_is_zero_without_activity() {
        assert_eq!(current_streak(&BTreeSet::new(), date("2026-09-24")), 0);
    }

    #[test]
    fn longest_streak_finds_best_window() {
        let days = set(&[
            "2026-09-01",
            "2026-09-02",
            "2026-09-03",
            "2026-09-10",
            "2026-09-11",
        ]);
        assert_eq!(longest_streak(&days), 3);
    }

    #[test]
    fn longest_streak_of_empty_history_is_zero() {
        assert_eq!(longest_streak(&BTreeSet::new()), 0);
    }

    #[test]
    fn accuracy_percentage() {
        assert_eq!(accuracy_percent(10, 8), 80.0);
        assert_eq!(accuracy_percent(0, 0), 0.0);
        // Некорректные данные не должны ломать расчёт: значение ограничивается 100%.
        assert_eq!(accuracy_percent(3, 10), 100.0);
    }

    #[test]
    fn mastery_grows_with_repetitions() {
        assert_eq!(mastery_percent(0), 0.0);
        assert_eq!(mastery_percent(1), 20.0);
        assert_eq!(mastery_percent(5), 100.0);
        assert_eq!(mastery_percent(50), 100.0);
    }

    #[test]
    fn interval_formatting_reads_naturally() {
        assert_eq!(format_interval(0), "сейчас");
        assert_eq!(format_interval(1), "через 1 день");
        assert_eq!(format_interval(6), "через 6 дней");
        assert!(format_interval(90).contains("мес."));
    }

    #[test]
    fn level_starts_at_one() {
        let progress = level_progress(0);
        assert_eq!(progress.level, 1);
        assert_eq!(progress.in_level, 0);
        assert_eq!(progress.needed, 100);
        assert_eq!(progress.percent, 0);
    }

    #[test]
    fn level_advances_when_threshold_reached() {
        let progress = level_progress(100);
        assert_eq!(progress.level, 2);
        assert_eq!(progress.in_level, 0);
        assert_eq!(progress.needed, 150);
        assert_eq!(progress.percent, 0);
    }

    #[test]
    fn level_progress_in_middle() {
        let progress = level_progress(175);
        assert_eq!(progress.level, 2);
        assert_eq!(progress.in_level, 75);
        assert_eq!(progress.needed, 150);
        assert_eq!(progress.percent, 50);
    }

    #[test]
    fn negative_xp_is_clamped() {
        assert_eq!(level_progress(-50).level, 1);
    }

    #[test]
    fn xp_rules_match_expected_rewards() {
        assert_eq!(review_xp(5), XP_REVIEW_SUCCESS);
        assert_eq!(review_xp(3), XP_REVIEW_SUCCESS);
        assert_eq!(review_xp(1), XP_REVIEW_FAIL);
        assert_eq!(grammar_xp(true), XP_GRAMMAR_CORRECT);
        assert_eq!(grammar_xp(false), XP_GRAMMAR_WRONG);
    }

    #[test]
    fn achievements_unlock_by_thresholds() {
        let input = AchievementInput {
            cards: 10,
            reviews: 50,
            successful: 48,
            current_streak: 7,
            grammar_correct: 20,
            best_combo: 10,
            ..Default::default()
        };
        let list = achievements(&input);

        let achieved: Vec<&str> = list
            .iter()
            .filter(|item| item.achieved)
            .map(|item| item.id)
            .collect();

        assert!(achieved.contains(&"first-word"));
        assert!(achieved.contains(&"ten-words"));
        assert!(achieved.contains(&"fifty-reviews"));
        assert!(achieved.contains(&"week-streak"));
        assert!(achieved.contains(&"sharp-mind"), "точность 96%");
        assert!(achieved.contains(&"combo-10"));
        assert!(achieved.contains(&"grammar-20"));
        assert!(!achieved.contains(&"hundred-words"));
        assert!(!achieved.contains(&"month-streak"));
        assert!(!achieved.contains(&"weak-fixed"));
    }

    #[test]
    fn accuracy_achievement_requires_enough_reviews() {
        let input = AchievementInput {
            reviews: 10,
            successful: 10,
            ..Default::default()
        };
        let list = achievements(&input);
        // Пункт есть в списке всегда, но получен он только при 20+ ответах.
        assert!(list.iter().any(|item| item.id == "sharp-mind"));
        assert!(
            !list
                .iter()
                .any(|item| item.id == "sharp-mind" && item.achieved)
        );
    }

    #[test]
    fn achievement_list_is_stable() {
        let list = achievements(&AchievementInput::default());
        assert_eq!(list.len(), 12);
        assert_eq!(list[0].id, "first-word");
        assert!(list.iter().all(|item| !item.achieved));
    }
}
