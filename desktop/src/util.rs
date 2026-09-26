//! Форматирование даты и времени без внешних зависимостей.

use std::time::{SystemTime, UNIX_EPOCH};

/// Секунды с 1 января 1970 года.
pub fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or_default()
}

/// Дата и время для отчётов: `2026-09-26T14:23:05Z`.
pub fn now_iso() -> String {
    let (year, month, day, hour, minute, second) = parts(unix_seconds());
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Имя файла с датой: `2026-09-26-142305`.
pub fn now_stamp() -> String {
    let (year, month, day, hour, minute, second) = parts(unix_seconds());
    format!("{year:04}-{month:02}-{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Разлагает Unix-время на календарные части (UTC).
fn parts(seconds: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (rest / 3_600) as u32,
        ((rest % 3_600) / 60) as u32,
        (rest % 60) as u32,
    )
}

/// Гражданская дата из дней с 1970 года (алгоритм Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_part + 2) / 5 + 1) as u32;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    } as u32;
    let year = if month <= 2 {
        year_of_era + era * 400 + 1
    } else {
        year_of_era + era * 400
    };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_epoch() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn converts_leap_day() {
        // 2000-02-29T00:00:00Z
        assert_eq!(parts(951_782_400), (2000, 2, 29, 0, 0, 0));
    }

    #[test]
    fn converts_current_date() {
        // 2026-09-26T00:00:00Z
        assert_eq!(parts(1_790_380_800), (2026, 9, 26, 0, 0, 0));
    }

    #[test]
    fn keeps_time_of_day() {
        // 2026-09-26T14:23:05Z
        assert_eq!(parts(1_790_432_585), (2026, 9, 26, 14, 23, 5));
    }
}
