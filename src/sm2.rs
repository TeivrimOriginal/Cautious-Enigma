//! Реализация алгоритма интервальных повторений SM-2
//! (SuperMemo-2, P.A. Wozniak).
//!
//! Качество ответа `quality` лежит в диапазоне 0..=5:
//! - 0..=2 — карточка забыта (нужно повторить)
//! - 3..=5 — карточка вспомнена (3 — тяжело, 4 — нормально, 5 — легко)

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

/// Минимально допустимый фактор лёгкости (EF).
pub const MIN_EASE: f64 = 1.3;

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
    /// Применяет SM-2 к текущему состоянию.
    ///
    /// `today` — дата повторения, `quality` — качество ответа (0..=5).
    /// Значения `quality` вне диапазона прижимаются к границам.
    pub fn review(&self, quality: u8, today: NaiveDate) -> (Self, ReviewOutcome) {
        let quality: f64 = f64::from(quality.clamp(0, 5));

        let mut repetitions = self.repetitions;
        let interval_days = if quality >= 3.0 {
            repetitions += 1;
            match repetitions {
                1 => 1,
                2 => 6,
                _ => (self.interval_days as f64 * self.ease).round().max(1.0) as u32,
            }
        } else {
            repetitions = 0;
            1
        };

        // Формула коррекции EF из статьи Wozniak.
        let ease = self.ease + 0.1 - (5.0 - quality) * (0.08 + (5.0 - quality) * 0.02);
        let ease = ease.max(MIN_EASE);

        let next_due = today + Days::new(u64::from(interval_days));
        let next = Self {
            repetitions,
            interval_days,
            ease,
        };
        let outcome = ReviewOutcome {
            next_interval: interval_days,
            next_due,
        };
        (next, outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day(ymd: &str) -> NaiveDate {
        NaiveDate::parse_from_str(ymd, "%Y-%m-%d").unwrap()
    }

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
}
