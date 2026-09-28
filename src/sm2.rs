//! Реализация алгоритма интервальных повторений SM-2
//! (SuperMemo-2, P.A. Wozniak).
//!
//! Качество ответа `quality` лежит в диапазоне 0..=5:
//! - 0..=2 — карточка забыта (нужно повторить)
//! - 3..=5 — карточка вспомнена (3 — тяжело, 4 — нормально, 5 — легко)
//!
//! Функция [`Sm2State::review`] — единственное место, где живёт математика
//! алгоритма. Она не ходит в базу и не знает про HTTP, поэтому её можно
//! покрыть обычными юнит-тестами: см. модуль `tests` в конце файла.

use chrono::{NaiveDate, TimeDelta};
use serde::{Deserialize, Serialize};

use crate::models::CardRow;

/// Минимально допустимый фактор лёгкости (EF).
pub const MIN_EASE: f64 = 1.3;

/// Максимальный интервал между повторениями — 100 лет.
///
/// В оригинальном SM-2 интервал растёт геометрически и ничем не ограничен:
/// при 25 ответах подряд «легко» он уже переполняет `i32` в базе, а
/// `NaiveDate + Days` на таком числе дней паникует. Ограничение в 100 лет
/// не меняет поведение ни для одной реальной карточки, но делает расчёт
/// тотальным: переполнения больше невозможно.
pub const MAX_INTERVAL_DAYS: u32 = 100 * 365;

/// Текущее состояние карточки в алгоритме SM-2.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Sm2State {
    /// Успешные повторения подряд.
    pub repetitions: u32,
    /// Текущий интервал повторения в днях.
    pub interval_days: u32,
    /// Фактор лёгкости (ease factor).
    pub ease: f64,
}

impl Default for Sm2State {
    fn default() -> Self {
        Self {
            repetitions: 0,
            interval_days: 0,
            ease: 2.5,
        }
    }
}

/// Итог одного повторения: новое состояние + когда показывать карточку дальше.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewOutcome {
    pub next_interval: u32,
    pub next_due: NaiveDate,
}

impl Sm2State {
    /// Состояние SM-2 по строке из базы.
    ///
    /// Значения, которые не могли появиться честным путём (отрицательные
    /// счётчики, EF ниже минимума, NaN), приводятся к допустимым: база
    /// пришла извне, и падать на ней нельзя.
    pub fn from_card(card: &CardRow) -> Self {
        Self {
            repetitions: card.repetitions.max(0) as u32,
            interval_days: card.interval_days.max(0) as u32,
            ease: sanitize_ease(card.ease),
        }
    }

    /// Применяет SM-2 к текущему состоянию.
    ///
    /// `today` — дата повторения, `quality` — качество ответа (0..=5).
    /// Значения `quality` вне диапазона прижимаются к границам.
    pub fn review(&self, quality: u8, today: NaiveDate) -> (Self, ReviewOutcome) {
        // `quality` приходит из формы и из JSON: прижимаем сверху, потому
        // что u8 меньше нуля не бывает, а 6..=255 — это «легко» из браузера.
        let quality = quality.min(5);
        let ease = sanitize_ease(self.ease);

        let mut repetitions = 0_u32;
        let mut interval_days = 1_u32;

        if quality >= 3 {
            // saturating_add, а не `+= 1`: состояние из базы не должно
            // ронять процесс на 4 294 967 295 повторениях.
            repetitions = self.repetitions.saturating_add(1);
            interval_days = match repetitions {
                1 => 1,
                2 => 6,
                _ => scaled_interval(self.interval_days, ease),
            };
        }

        let interval_days = interval_days.clamp(1, MAX_INTERVAL_DAYS);
        let next_due = add_days(today, interval_days);

        (
            Self {
                repetitions,
                interval_days,
                ease: adjusted_ease(ease, quality),
            },
            ReviewOutcome {
                next_interval: interval_days,
                next_due,
            },
        )
    }
}

/// Следующий интервал: предыдущий, умноженный на фактор лёгкости.
fn scaled_interval(interval_days: u32, ease: f64) -> u32 {
    let scaled = f64::from(interval_days.max(1)) * ease;
    // `ease` приходит из базы: NaN и бесконечность не должны превращаться
    // в нулевой интервал (иначе карточка зациклится на «сегодня»).
    if !scaled.is_finite() || scaled < 1.0 {
        return 1;
    }
    scaled.min(f64::from(MAX_INTERVAL_DAYS)).round() as u32
}

/// Формула коррекции EF из статьи Wozniak.
fn adjusted_ease(ease: f64, quality: u8) -> f64 {
    let q = f64::from(quality);
    let next = ease + 0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02);
    sanitize_ease(next)
}

/// EF не может быть ниже минимума и не может быть «не числом».
fn sanitize_ease(ease: f64) -> f64 {
    if ease.is_finite() {
        ease.max(MIN_EASE)
    } else {
        MIN_EASE
    }
}

/// Сдвиг даты без паники: за пределами диапазона chrono отдаём последнюю
/// доступную дату — карточка просто уйдёт в конец очереди.
fn add_days(today: NaiveDate, days: u32) -> NaiveDate {
    today
        .checked_add_signed(TimeDelta::days(i64::from(days)))
        .unwrap_or(NaiveDate::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(ymd: &str) -> NaiveDate {
        NaiveDate::parse_from_str(ymd, "%Y-%m-%d").unwrap()
    }

    fn card(repetitions: i32, interval_days: i32, ease: f64) -> CardRow {
        CardRow {
            id: 1,
            front: "word".into(),
            back: "слово".into(),
            example: None,
            repetitions,
            interval_days,
            ease,
            due_date: None,
            created_at: chrono::DateTime::default(),
            last_reviewed_at: None,
        }
    }

    /// Прогоняет `steps` повторений подряд с одной оценкой.
    fn run(start: Sm2State, quality: u8, today: NaiveDate, steps: u32) -> Sm2State {
        let mut state = start;
        let mut day = today;
        for _ in 0..steps {
            let (next, outcome) = state.review(quality, day);
            state = next;
            day = outcome.next_due;
        }
        state
    }

    /* ---------------------------------------------------------------- *
     * Начальное состояние
     * ---------------------------------------------------------------- */

    #[test]
    fn new_card_starts_from_scratch() {
        let state = Sm2State::default();
        assert_eq!(state.repetitions, 0);
        assert_eq!(state.interval_days, 0);
        assert!((state.ease - 2.5).abs() < 1e-9);
    }

    /* ---------------------------------------------------------------- *
     * Интервалы
     * ---------------------------------------------------------------- */

    #[test]
    fn new_card_quality_5() {
        let state = Sm2State::default();
        let today = day("2026-09-24");
        let (next, outcome) = state.review(5, today);

        assert_eq!(next.repetitions, 1);
        assert_eq!(next.interval_days, 1);
        assert!((next.ease - 2.6).abs() < 1e-9);
        assert_eq!(outcome.next_due, day("2026-09-25"));
    }

    #[test]
    fn first_success_always_means_one_day() {
        for quality in 3..=5 {
            let (next, outcome) = Sm2State::default().review(quality, day("2026-09-24"));
            assert_eq!(next.interval_days, 1, "quality={quality}");
            assert_eq!(outcome.next_due, day("2026-09-25"), "quality={quality}");
        }
    }

    #[test]
    fn second_success_uses_six_days() {
        let after_first = Sm2State {
            repetitions: 1,
            interval_days: 1,
            ease: 2.6,
        };
        let (next, _) = after_first.review(5, day("2026-09-25"));

        assert_eq!(next.repetitions, 2);
        assert_eq!(next.interval_days, 6);
        assert!((next.ease - 2.7).abs() < 1e-9);
    }

    #[test]
    fn second_success_survives_a_ruined_interval() {
        // Карточка с интервалом 200 дней, но с одной репетицией: по
        // SM-2 второе успешное повторение всё равно даёт ровно 6 дней.
        let state = Sm2State {
            repetitions: 1,
            interval_days: 200,
            ease: 2.5,
        };
        let (next, _) = state.review(4, day("2026-09-24"));
        assert_eq!(next.interval_days, 6);
    }

    #[test]
    fn later_reviews_multiply_by_ease() {
        let state = Sm2State {
            repetitions: 3,
            interval_days: 15,
            ease: 2.5,
        };
        let (next, outcome) = state.review(4, day("2026-10-01"));

        // 15 * 2.5 = 37.5 -> 38 (округление)
        assert_eq!(next.interval_days, 38);
        assert_eq!(outcome.next_interval, 38);
        assert_eq!(outcome.next_due, day("2026-11-08"));
        // Оценка 4 («хорошо») по формуле Wozniak не меняет EF:
        // 0.1 - 1 * (0.08 + 1 * 0.02) = 0
        assert!((next.ease - 2.5).abs() < 1e-9);
    }

    #[test]
    fn interval_rounding_matches_the_formula() {
        // 6 * EF: ровно целое, вниз и вверх от .5.
        let cases = [
            (2.5, 15_u32),
            (2.4, 14), // 14.4 -> 14
            (2.416_666_666_666_666_5, 15), // 14.5 -> 15
            (2.6, 16), // 15.6 -> 16
            (1.3, 8),  // 7.8 -> 8
        ];
        for (ease, expected) in cases {
            let state = Sm2State {
                repetitions: 2,
                interval_days: 6,
                ease,
            };
            let (next, _) = state.review(4, day("2026-09-24"));
            assert_eq!(next.interval_days, expected, "ease={ease}");
        }
    }

    #[test]
    fn long_perfect_run_matches_the_reference_table() {
        // Оригинальный SM-2: I(1)=1, I(2)=6, I(n)=I(n-1)*EF, EF растёт на 0.1.
        // Дальше 12 863 дней включительный предел 100 лет наступает на
        // одиннадцатом повторении — это проверяется отдельным тестом.
        let expected_intervals = [1_u32, 6, 16, 45, 131, 393, 1_218, 3_898, 12_863];
        let mut state = Sm2State::default();
        let today = day("2026-09-24");

        for (step, expected) in expected_intervals.iter().enumerate() {
            let (next, _) = state.review(5, today);
            assert_eq!(next.interval_days, *expected, "повторение №{}", step + 1);
            assert_eq!(next.repetitions, step as u32 + 1);
            state = next;
        }
        // 9-е повторение подряд: EF 2.5 + 9 * 0.1 = 3.4
        assert!((state.ease - 3.4).abs() < 1e-9);
    }

    #[test]
    fn the_eleventh_perfect_review_hits_the_interval_ceiling() {
        // 12 863 * 3.5 = 45 020 дней — уже больше ста лет, поэтому
        // интервал останавливается на 36 500.
        let mut state = Sm2State::default();
        let today = day("2026-09-24");
        for _ in 0..9 {
            let (next, _) = state.review(5, today);
            state = next;
        }
        assert_eq!(state.interval_days, 12_863);

        let (next, outcome) = state.review(5, today);
        assert_eq!(next.interval_days, MAX_INTERVAL_DAYS);
        assert_eq!(outcome.next_interval, MAX_INTERVAL_DAYS);
        // Фактор лёгкости продолжает расти: ограничен только интервал.
        assert!((next.ease - 3.5).abs() < 1e-9);
    }

    #[test]
    fn interval_never_shrinks_below_one() {
        let state = Sm2State {
            repetitions: 2,
            interval_days: 6,
            ease: MIN_EASE,
        };
        // даже при огромном интервале * минимальном EF результат >= 1
        let (next, _) = state.review(3, day("2026-09-24"));
        assert!(next.interval_days >= 1);
    }

    #[test]
    fn interval_is_capped_to_stay_inside_date_range() {
        let state = Sm2State {
            repetitions: 40,
            interval_days: 1_000_000,
            ease: 12.0,
        };
        let (next, outcome) = state.review(5, day("2026-09-24"));
        assert_eq!(next.interval_days, MAX_INTERVAL_DAYS);
        assert_eq!(outcome.next_interval, MAX_INTERVAL_DAYS);
    }

    #[test]
    fn every_quality_keeps_the_interval_in_range() {
        let today = day("2026-09-24");
        for quality in 0..=255 {
            for start in [1_u32, 6, 365, MAX_INTERVAL_DAYS] {
                let state = Sm2State {
                    repetitions: 7,
                    interval_days: start,
                    ease: 2.5,
                };
                let (next, outcome) = state.review(quality, today);
                assert!(
                    (1..=MAX_INTERVAL_DAYS).contains(&next.interval_days),
                    "quality={quality} start={start} -> {}",
                    next.interval_days
                );
                assert_eq!(outcome.next_interval, next.interval_days);
                assert!(outcome.next_due > today);
            }
        }
    }

    #[test]
    fn interval_never_overflows_the_database_column() {
        // interval_days в базе — INTEGER. Долгое повторение «легко» не
        // должно приводить к отрицательному значению при записи в базу.
        let state = Sm2State::default();
        let final_state = run(state, 5, day("2026-09-24"), 30);
        assert!(final_state.interval_days <= MAX_INTERVAL_DAYS);
        assert!(i32::try_from(final_state.interval_days).is_ok());
        assert!(i32::try_from(final_state.repetitions).is_ok());
    }

    /* ---------------------------------------------------------------- *
     * Множитель лёгкости
     * ---------------------------------------------------------------- */

    #[test]
    fn ease_changes_match_the_reference_table() {
        // EF' = EF + 0.1 - (5 - q) * (0.08 + (5 - q) * 0.02)
        let cases = [
            (5_u8, 2.6),
            (4, 2.5),
            (3, 2.36),
            (2, 2.18),
            (1, 1.96),
            (0, 1.70),
        ];
        for (quality, expected) in cases {
            let (next, _) = Sm2State::default().review(quality, day("2026-09-24"));
            assert!(
                (next.ease - expected).abs() < 1e-9,
                "quality={quality}: {} != {expected}",
                next.ease
            );
        }
    }

    #[test]
    fn ease_never_drops_below_min() {
        let state = Sm2State {
            repetitions: 0,
            interval_days: 1,
            ease: 1.5,
        };
        let (next, _) = state.review(0, day("2026-09-24"));
        assert!((next.ease - MIN_EASE).abs() < 1e-9);
    }

    #[test]
    fn ease_sits_on_the_floor_after_a_series_of_failures() {
        let mut state = Sm2State {
            repetitions: 3,
            interval_days: 40,
            ease: 2.0,
        };
        for _ in 0..5 {
            let (next, _) = state.review(0, day("2026-09-24"));
            state = next;
        }
        assert!((state.ease - MIN_EASE).abs() < 1e-9);
        assert_eq!(state.repetitions, 0);
    }

    #[test]
    fn ease_above_the_floor_grows_with_easy_answers() {
        let state = Sm2State {
            repetitions: 2,
            interval_days: 6,
            ease: 2.5,
        };
        let (next, _) = state.review(5, day("2026-09-24"));
        assert!((next.ease - 2.6).abs() < 1e-9);
        assert!(next.ease > state.ease);
    }

    #[test]
    fn corrupted_ease_from_the_database_is_repaired() {
        // В базе возможен любой DOUBLE PRECISION: NaN проходит проверку
        // `ease >= 1.3` в PostgreSQL, потому что NaN больше всего.
        for broken in [f64::NAN, f64::INFINITY, 0.0, -5.0, 1.0] {
            let (next, outcome) = Sm2State {
                repetitions: 3,
                interval_days: 10,
                ease: broken,
            }
            .review(4, day("2026-09-24"));
            assert!(
                next.ease.is_finite() && next.ease >= MIN_EASE,
                "ease={broken} -> {}",
                next.ease
            );
            assert!(outcome.next_interval >= 1);
            assert!(outcome.next_due > day("2026-09-24"));
        }
    }

    /* ---------------------------------------------------------------- *
     * Репетиции и сброс после провала
     * ---------------------------------------------------------------- */

    #[test]
    fn failed_review_resets_repetitions() {
        let state = Sm2State {
            repetitions: 5,
            interval_days: 60,
            ease: 2.5,
        };
        let (next, outcome) = state.review(1, day("2026-09-24"));

        assert_eq!(next.repetitions, 0);
        assert_eq!(next.interval_days, 1);
        assert_eq!(outcome.next_due, day("2026-09-25"));
        // 1 -> +0.1 - 4*(0.08 + 4*0.02) = 0.1 - 0.64 = -0.54
        assert!((next.ease - 1.96).abs() < 1e-9);
    }

    #[test]
    fn every_failing_quality_resets_the_streak() {
        for quality in 0..=2 {
            let (next, outcome) = Sm2State {
                repetitions: 9,
                interval_days: 300,
                ease: 2.5,
            }
            .review(quality, day("2026-09-24"));
            assert_eq!(next.repetitions, 0, "quality={quality}");
            assert_eq!(next.interval_days, 1, "quality={quality}");
            assert_eq!(outcome.next_interval, 1, "quality={quality}");
        }
    }

    #[test]
    fn failure_then_success_starts_the_ladder_again() {
        let broken = Sm2State {
            repetitions: 4,
            interval_days: 90,
            ease: 2.5,
        };
        let (after_fail, _) = broken.review(2, day("2026-09-24"));
        let (after_success, _) = after_fail.review(4, day("2026-09-25"));
        assert_eq!(after_success.repetitions, 1);
        assert_eq!(after_success.interval_days, 1, "лестница начинается заново");
    }

    #[test]
    fn quality_is_clamped() {
        let state = Sm2State::default();
        let (from_high, _) = state.review(255, day("2026-09-24"));
        let (from_low, _) = state.review(0, day("2026-09-24"));
        // 255 -> 5: репетиции 1, интервал 1
        assert_eq!(from_high.repetitions, 1);
        // 0 -> 0: репетиции сброшены
        assert_eq!(from_low.repetitions, 0);
    }

    #[test]
    fn quality_above_five_behaves_exactly_like_five() {
        let start = Sm2State {
            repetitions: 2,
            interval_days: 6,
            ease: 2.5,
        };
        let (five, _) = start.review(5, day("2026-09-24"));
        for quality in 6..=255 {
            let (other, _) = start.review(quality, day("2026-09-24"));
            assert_eq!(other, five, "quality={quality}");
        }
    }

    #[test]
    fn repetition_counter_does_not_overflow() {
        // Счётчик приходит из базы (INTEGER). Даже если там мусор,
        // процесс падать не должен.
        let (next, _) = Sm2State {
            repetitions: u32::MAX,
            interval_days: 10,
            ease: 2.5,
        }
        .review(4, day("2026-09-24"));
        assert_eq!(next.repetitions, u32::MAX);
    }

    /* ---------------------------------------------------------------- *
     * Даты
     * ---------------------------------------------------------------- */

    #[test]
    fn next_due_always_equals_today_plus_interval() {
        let today = day("2026-09-24");
        let state = Sm2State {
            repetitions: 4,
            interval_days: 30,
            ease: 2.6,
        };
        for quality in 0..=5 {
            let (next, outcome) = state.review(quality, today);
            assert_eq!(
                outcome.next_due,
                today + chrono::Days::new(u64::from(outcome.next_interval)),
                "quality={quality}"
            );
            assert!(outcome.next_due > today, "quality={quality}");
            assert_eq!(next.interval_days, outcome.next_interval);
        }
    }

    #[test]
    fn next_due_crosses_month_and_year_boundaries() {
        let (next, outcome) = Sm2State::default().review(5, day("2026-12-31"));
        assert_eq!(next.interval_days, 1);
        assert_eq!(outcome.next_due, day("2027-01-01"));

        let (next, outcome) = Sm2State {
            repetitions: 1,
            interval_days: 1,
            ease: 2.5,
        }
        .review(5, day("2026-01-31"));
        assert_eq!(next.interval_days, 6);
        assert_eq!(outcome.next_due, day("2026-02-06"));
    }

    #[test]
    fn next_due_handles_leap_day() {
        // 2028 — високосный год.
        let (next, outcome) = Sm2State::default().review(5, day("2028-02-28"));
        assert_eq!(next.interval_days, 1);
        assert_eq!(outcome.next_due, day("2028-02-29"));

        let (next, outcome) = Sm2State {
            repetitions: 1,
            interval_days: 1,
            ease: 2.5,
        }
        .review(5, day("2028-02-28"));
        assert_eq!(next.interval_days, 6);
        assert_eq!(outcome.next_due, day("2028-03-05"), "февраль 2028 — 29 дней");
    }

    #[test]
    fn extreme_state_does_not_panic() {
        // Раньше `NaiveDate + Days` паниковал на переполнении: карточка
        // с огромным интервалом роняла весь запрос.
        let today = day("2026-09-24");
        let states = [
            Sm2State {
                repetitions: 30,
                interval_days: u32::MAX,
                ease: f64::MAX,
            },
            Sm2State {
                repetitions: 30,
                interval_days: u32::MAX,
                ease: 1e300,
            },
        ];
        for state in states {
            let (next, outcome) = state.review(5, today);
            assert!((1..=MAX_INTERVAL_DAYS).contains(&next.interval_days));
            assert!(outcome.next_due >= today);
        }
    }

    #[test]
    fn review_on_the_last_representable_day_does_not_panic() {
        let (next, outcome) = Sm2State {
            repetitions: 5,
            interval_days: 100,
            ease: 2.5,
        }
        .review(5, NaiveDate::MAX);
        assert!(next.interval_days >= 1);
        assert_eq!(outcome.next_due, NaiveDate::MAX);
    }

    #[test]
    fn add_days_clamps_instead_of_panicking() {
        assert_eq!(add_days(day("2026-09-24"), 0), day("2026-09-24"));
        assert_eq!(add_days(day("2026-09-24"), 1), day("2026-09-25"));
        assert_eq!(add_days(NaiveDate::MAX, 1), NaiveDate::MAX);
        assert_eq!(add_days(NaiveDate::MIN, 1), NaiveDate::MIN.succ_opt().unwrap());
    }

    /* ---------------------------------------------------------------- *
     * Инварианты
     * ---------------------------------------------------------------- */

    #[test]
    fn review_is_pure() {
        let state = Sm2State {
            repetitions: 3,
            interval_days: 20,
            ease: 2.3,
        };
        let first = state.review(4, day("2026-09-24"));
        let second = state.review(4, day("2026-09-24"));
        assert_eq!(first, second);
        // Исходное состояние не меняется: это позволяет считать прогноз
        // на копиях (см. `stats::forecast`).
        assert_eq!(state.repetitions, 3);
        assert_eq!(state.interval_days, 20);
        assert!((state.ease - 2.3).abs() < 1e-9);
    }

    #[test]
    fn success_never_shortens_the_interval_after_the_first_week() {
        let mut state = Sm2State {
            repetitions: 2,
            interval_days: 6,
            ease: MIN_EASE,
        };
        let mut day = day("2026-09-24");
        for _ in 0..8 {
            let previous = state.interval_days;
            let (next, outcome) = state.review(3, day);
            assert!(
                next.interval_days >= previous,
                "интервал уменьшился: {previous} -> {}",
                next.interval_days
            );
            state = next;
            day = outcome.next_due;
        }
    }

    #[test]
    fn mixed_answers_keep_every_value_valid() {
        let mut state = Sm2State::default();
        let mut day = day("2026-09-24");
        for step in 0..60 {
            let quality = [5_u8, 3, 4, 1, 5, 0, 2, 4][step % 8];
            let (next, outcome) = state.review(quality, day);
            assert!(next.ease >= MIN_EASE && next.ease.is_finite());
            assert!((1..=MAX_INTERVAL_DAYS).contains(&next.interval_days));
            assert!(outcome.next_due > day);
            state = next;
            day = outcome.next_due;
        }
        assert!(state.repetitions <= 60);
    }

    /// Полный перебор пространства состояний, до которого может дойти
    /// приложение: границы EF, границы интервала, все оценки.
    ///
    /// Один такой тест находит ошибки, которые табличные кейсы пропускают,
    /// потому что он проверяет инвариант, а не конкретное число.
    #[test]
    fn state_space_keeps_every_invariant() {
        let easies = [
            MIN_EASE,
            1.5,
            2.5,
            3.0,
            10.0,
            f64::MAX,
            f64::MIN_POSITIVE,
            f64::NAN,
            f64::INFINITY,
            0.0,
            -1.0,
        ];
        let intervals = [0_u32, 1, 6, 100, 10_000, u32::MAX];
        let repetitions = [0_u32, 1, 2, 7, u32::MAX];
        let today = day("2026-09-24");

        for ease in easies {
            for interval_days in intervals {
                for repetitions in repetitions {
                    let state = Sm2State {
                        repetitions,
                        interval_days,
                        ease,
                    };
                    for quality in 0..=8 {
                        let (next, outcome) = state.review(quality, today);

                        assert!(
                            next.ease.is_finite(),
                            "ease={ease} interval={interval_days} q={quality} -> ease {}",
                            next.ease
                        );
                        assert!(next.ease >= MIN_EASE);
                        assert!(
                            (1..=MAX_INTERVAL_DAYS).contains(&next.interval_days),
                            "interval={interval_days} q={quality} -> {}",
                            next.interval_days
                        );
                        assert_eq!(outcome.next_interval, next.interval_days);
                        assert!(outcome.next_due > today);
                        assert!(next.repetitions <= repetitions.saturating_add(1));

                        // Оценка выше 5 — это ровно 5.
                        let (clamped, _) = state.review(quality.min(5), today);
                        assert_eq!(clamped, next, "q={quality} должен прижиматься к 5");
                    }
                }
            }
        }
    }

    /* ---------------------------------------------------------------- *
     * Состояние из строки базы
     * ---------------------------------------------------------------- */

    #[test]
    fn state_is_read_from_the_database_row() {
        let state = Sm2State::from_card(&card(4, 30, 2.5));
        assert_eq!(state.repetitions, 4);
        assert_eq!(state.interval_days, 30);
        assert!((state.ease - 2.5).abs() < 1e-9);
    }

    #[test]
    fn negative_columns_from_the_database_are_repaired() {
        let state = Sm2State::from_card(&card(-3, -7, 2.5));
        assert_eq!(state.repetitions, 0);
        assert_eq!(state.interval_days, 0);
        let (next, _) = state.review(4, day("2026-09-24"));
        assert_eq!(next.repetitions, 1);
        assert_eq!(next.interval_days, 1);
    }

    #[test]
    fn ease_column_below_the_floor_is_lifted() {
        let state = Sm2State::from_card(&card(2, 6, 0.4));
        assert!((state.ease - MIN_EASE).abs() < 1e-9);
    }

    #[test]
    fn round_trip_through_the_database_row() {
        let start = Sm2State {
            repetitions: 3,
            interval_days: 15,
            ease: 2.5,
        };
        let (next, _) = start.review(5, day("2026-09-24"));
        let row = card(
            next.repetitions as i32,
            next.interval_days as i32,
            next.ease,
        );
        assert_eq!(Sm2State::from_card(&row), next);
    }
}
