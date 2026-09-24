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
}
