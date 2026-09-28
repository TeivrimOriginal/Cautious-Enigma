//! Расчёты статистики. Вынесены в отдельный модуль без зависимостей от БД,
//! поэтому логику стриков и точности можно покрыть обычными юнит-тестами.

use std::collections::BTreeSet;

use chrono::NaiveDate;

use crate::sm2::Sm2State;

/// Текущий стрик: дни подряд с хотя бы одним повторением.
///
/// Стрик не прерывается, если сегодня повторений ещё не было, но вчера были:
/// иначе стрик обнулялся бы каждое утро до первого повторения.
pub fn current_streak(days: &BTreeSet<NaiveDate>, today: NaiveDate) -> u32 {
    // `today` приходит из `Utc::now()`, но функция публичная и зовётся из
    // тестов с произвольными датами, включая крайние: паниковать здесь
    // нельзя, отсутствие «вчера» — просто отсутствие стрика.
    let yesterday = today.pred_opt();
    let start = match days.get(&today) {
        Some(_) => Some(today),
        None => yesterday.and_then(|yesterday| days.get(&yesterday).map(|_| yesterday)),
    };
    let Some(start) = start else {
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
///
/// Падежи считаются явно: в диапазоне 21..=60 месяцев округление вниз
/// («через 0 мес.») выглядело бы как ошибка, поэтому месяц считается как
/// округлённое вверх значение, а годы — с одним знаком после запятой.
pub fn format_interval(days: i32) -> String {
    match days {
        d if d <= 0 => "сейчас".to_string(),
        1 => "через 1 день".to_string(),
        2..=4 => format!("через {days} дня"),
        5..=20 => format!("через {days} дней"),
        21..=365 => {
            // Округление вверх: `30 / 30 = 0` превращалось в «через 0 мес.».
            let months = days / 30 + i32::from(days % 30 > 0);
            format!("через {months} мес.")
        }
        _ => {
            let years = days as f64 / 365.0;
            format!("через {years:.1} года")
        }
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

/* ------------------------------------------------------------------ *
 * Прогноз освоения
 * ------------------------------------------------------------------ */

/// Состояние карточки для симуляции: только то, что влияет на SM-2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimCard {
    pub repetitions: u32,
    pub interval_days: u32,
    pub ease: f64,
    pub due_date: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForecastPoint {
    pub day: NaiveDate,
    /// Сколько карточек накопилось с 3+ успешными повторениями.
    pub learned: i32,
    /// Сколько карточек достигло 5+ повторений.
    pub mastered: i32,
    /// Всего повторений с начала симуляции.
    pub reviews: i32,
}

/// Симулирует повторения на `days` дней вперёд при ответе `quality`.
///
/// Реальные карточки не меняются: симуляция идёт на копии.
pub fn forecast(cards: &[SimCard], today: NaiveDate, days: i32, quality: u8) -> Vec<ForecastPoint> {
    let mut simulated: Vec<SimCard> = cards.to_vec();
    let mut points = Vec::with_capacity(days.max(0) as usize + 1);
    let mut total_reviews = 0_i32;

    for offset in 0..=days.max(0) {
        let day = today + chrono::Days::new(u64::try_from(offset).unwrap_or(0));

        if offset > 0 {
            for card in simulated.iter_mut() {
                if card.due_date.is_some_and(|due| due <= day) {
                    let state = Sm2State {
                        repetitions: card.repetitions,
                        interval_days: card.interval_days,
                        ease: card.ease,
                    };
                    let (next, outcome) = state.review(quality, day);
                    card.repetitions = next.repetitions;
                    card.interval_days = next.interval_days;
                    card.ease = next.ease;
                    card.due_date = Some(outcome.next_due);
                    total_reviews += 1;
                }
            }
        }

        points.push(ForecastPoint {
            day,
            learned: simulated
                .iter()
                .filter(|card| card.repetitions >= 3)
                .count() as i32,
            mastered: simulated
                .iter()
                .filter(|card| card.repetitions >= 5)
                .count() as i32,
            reviews: total_reviews,
        });
    }

    points
}

/// Итоговый опыт по накопленным счётчикам.
///
/// Считается из того, что уже есть в базе, поэтому переживает пересчёт и
/// не зависит от того, где была поставлена галочка.
///
/// Счётчики приходят из `COUNT(*)` и могут быть сколь угодно большими,
/// поэтому сложение идёт в `i64`, а приведение к `i32` насыщающее: обычный
/// `as i32` перевернул бы значение и показал бы отрицательный опыт вместо
/// максимального уровня.
pub fn total_xp(
    successful_reviews: i64,
    failed_reviews: i64,
    cards: i64,
    grammar_total: i64,
    grammar_correct: i64,
) -> i32 {
    let grammar_wrong = (grammar_total - grammar_correct).max(0);
    let total = [
        (XP_REVIEW_SUCCESS, successful_reviews),
        (XP_REVIEW_FAIL, failed_reviews),
        (XP_NEW_CARD, cards),
        (XP_GRAMMAR_CORRECT, grammar_correct),
        (XP_GRAMMAR_WRONG, grammar_wrong),
    ]
    .into_iter()
    .fold(0_i64, |sum, (reward, count)| {
        sum.saturating_add(i64::from(reward).saturating_mul(count.max(0)))
    });
    i32::try_from(total).unwrap_or(i32::MAX)
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
    fn streak_survives_the_last_representable_day() {
        // `current_streak` брал предыдущий день через `expect`: на
        // `NaiveDate::MIN` это паника, то есть 500 на живом запросе.
        assert_eq!(current_streak(&set(&["2026-09-24"]), NaiveDate::MIN), 0);
        assert_eq!(current_streak(&BTreeSet::new(), NaiveDate::MIN), 0);
    }

    #[test]
    fn streak_counts_days_back_to_the_first_representable_one() {
        // Обход назад упирается в `NaiveDate::MIN` и обязан просто остановиться.
        let days = set(&["2026-09-24", "2026-09-23", "2026-09-22"]);
        assert_eq!(current_streak(&days, date("2026-09-24")), 3);
    }

    #[test]
    fn streak_ignores_days_far_in_the_past() {
        // Активность месячной давности не продлевает сегодняшний стрик.
        let days = set(&["2020-01-01", "2020-01-02", "2020-01-03"]);
        assert_eq!(current_streak(&days, date("2026-09-24")), 0);
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
    fn negative_interval_is_not_shown_as_a_wait() {
        // Отрицательный interval_days возможен только из битой базы, но
        // показывать пользователю «через -3 дня» нельзя.
        for days in [-1, -30, i32::MIN] {
            assert_eq!(format_interval(days), "сейчас", "days={days}");
        }
    }

    #[test]
    fn interval_never_rounds_down_to_zero_months() {
        // При округлении вниз 21 день превращался в «через 0 мес.».
        for days in 21..=60 {
            let text = format_interval(days);
            assert!(!text.contains("0 мес."), "days={days} -> {text}");
            assert!(text.contains("мес."), "days={days} -> {text}");
        }
    }

    #[test]
    fn every_interval_reads_as_a_positive_number() {
        for days in 0..=3_000 {
            let text = format_interval(days);
            assert!(!text.contains("-"), "days={days} -> {text}");
            assert!(!text.contains("через 0 "), "days={days} -> {text}");
        }
    }

    #[test]
    fn interval_past_a_year_is_shown_in_years() {
        let text = format_interval(730);
        assert!(text.contains("года"), "text={text}");
        assert!(format_interval(400).contains("года"));
    }

    #[test]
    fn mastery_percent_is_bounded() {
        // Приходит из базы: отрицательные и огромные значения не должны
        // давать ни NaN, ни проценты вне 0..=100.
        for repetitions in [i32::MIN, -5, 0, 1, 4, 5, 6, 1_000, i32::MAX] {
            let value = mastery_percent(repetitions);
            assert!(
                (0.0..=100.0).contains(&value),
                "repetitions={repetitions} -> {value}"
            );
        }
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
    fn level_progress_is_consistent_for_every_amount_of_xp() {
        // `in_level` всегда в пределах уровня, `percent` — в 0..=100,
        // и уровень растёт монотонно.
        let mut previous_level = 0;
        for xp in (0..5_000).chain([10_000, 100_000, 1_000_000]) {
            let progress = level_progress(xp);
            assert!(progress.level >= previous_level, "xp={xp}");
            assert!(
                (0..progress.needed).contains(&progress.in_level),
                "xp={xp} -> {progress:?}"
            );
            assert!(progress.percent <= 100, "xp={xp} -> {progress:?}");
            assert!(progress.level >= 1, "xp={xp}");
            previous_level = progress.level;
        }
    }

    #[test]
    fn level_titles_are_never_empty() {
        for level in 0..40 {
            assert!(!level_progress(level * 10_000).title().is_empty());
        }
        assert_eq!(level_progress(0).title(), "Новичок");
        assert_eq!(level_progress(1_000_000).title(), "Мастер");
    }

    #[test]
    fn total_xp_counts_every_source() {
        let xp = total_xp(10, 5, 20, 8, 6);
        assert_eq!(
            xp,
            10 * XP_REVIEW_SUCCESS
                + 5 * XP_REVIEW_FAIL
                + 20 * XP_NEW_CARD
                + 6 * XP_GRAMMAR_CORRECT
                + 2 * XP_GRAMMAR_WRONG
        );
    }

    #[test]
    fn total_xp_of_an_empty_profile_is_zero() {
        assert_eq!(total_xp(0, 0, 0, 0, 0), 0);
    }

    #[test]
    fn total_xp_never_overflows_into_a_negative_value() {
        // Счётчики приходят из COUNT(*); обычный `as i32` перевернул бы их
        // и показал бы отрицательный опыт.
        assert_eq!(total_xp(i64::MAX, i64::MAX, i64::MAX, i64::MAX, i64::MAX), i32::MAX);
        assert_eq!(total_xp(1_000_000, 0, 0, 0, 0), 10_000_000);
    }

    #[test]
    fn total_xp_ignores_negative_counters() {
        assert_eq!(total_xp(-100, -100, -100, 0, 0), 0);
        // grammar_correct больше, чем всего упражнений: отрицательная
        // разница не должна вычитаться из опыта.
        assert_eq!(total_xp(0, 0, 0, 2, 5), 5 * XP_GRAMMAR_CORRECT);
    }

    #[test]
    fn review_xp_matches_the_sm2_success_boundary() {
        // Граница 2/3 — та же, что в `sm2::Sm2State::review`: оценка 2
        // это провал, 3 — успех. Расхождение ломает всю экономику.
        for quality in 0..=2 {
            assert_eq!(review_xp(quality), XP_REVIEW_FAIL, "quality={quality}");
        }
        for quality in 3..=255 {
            assert_eq!(
                review_xp(quality),
                XP_REVIEW_SUCCESS,
                "quality={quality}"
            );
        }
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

    fn card(due: Option<&str>, repetitions: u32, interval: u32) -> SimCard {
        SimCard {
            repetitions,
            interval_days: interval,
            ease: 2.5,
            due_date: due.map(|value| {
                NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("корректная дата")
            }),
        }
    }

    #[test]
    fn forecast_without_cards_is_flat() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        let points = forecast(&[], today, 14, 4);
        assert_eq!(points.len(), 15);
        assert!(
            points
                .iter()
                .all(|point| point.learned == 0 && point.reviews == 0)
        );
    }

    #[test]
    fn forecast_grows_learned_words() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        let cards = vec![card(Some("2026-09-24"), 0, 0)];
        let points = forecast(&cards, today, 14, 4);

        // Расписание SM-2 при оценке 4: повторы на 1-й, 2-й, затем 8-й день.
        assert_eq!(points[0].learned, 0, "в первый день ещё не повторено");
        assert_eq!(points[1].learned, 0, "после одного повторения");
        assert_eq!(points[2].learned, 0, "после двух повторений");
        assert_eq!(points[8].learned, 1, "на восьмой день слово в работе");
        assert!(
            points
                .windows(2)
                .all(|pair| pair[0].learned <= pair[1].learned)
        );
        assert!(
            points
                .windows(2)
                .all(|pair| pair[0].reviews <= pair[1].reviews)
        );
    }

    #[test]
    fn forecast_respects_due_dates() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        // Карточка назначена на 25-е: сегодня её не трогаем.
        let cards = vec![card(Some("2026-09-25"), 0, 0)];
        let points = forecast(&cards, today, 1, 4);
        assert_eq!(points[0].reviews, 0);
        assert_eq!(points[1].reviews, 1);
    }

    #[test]
    fn forecast_does_not_mutate_input() {
        let today = NaiveDate::from_ymd_opt(2026, 9, 24).unwrap();
        let cards = vec![card(Some("2026-09-24"), 0, 0)];
        let before = cards.clone();
        forecast(&cards, today, 7, 4);
        assert_eq!(cards, before);
    }
}
