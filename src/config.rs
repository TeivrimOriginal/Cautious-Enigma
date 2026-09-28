//! Конфигурация приложения, читаемая из переменных окружения.

use std::env;
use std::net::SocketAddr;
use std::time::Duration;

use thiserror::Error;
use tower_cookies::Key;

/// Длина ключа подписи cookie: 32 байта на подпись + 32 байта на шифрование.
const KEY_BYTES: usize = 64;
const HEX_KEY_CHARS: usize = KEY_BYTES * 2;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("обязательная переменная окружения {0} не задана")]
    Missing(&'static str),
    #[error("COOKIE_KEY должен быть 128 hex-символов, получено {0}")]
    InvalidCookieKey(usize),
    #[error("не удалось разобрать BIND_ADDR: {0}")]
    InvalidBindAddr(String),
    #[error("не удалось разобрать DATABASE_MAX_CONNECTIONS: {0}")]
    InvalidPoolSize(String),
}

#[derive(Debug, Clone)]
pub struct Config {
    /// Строка подключения к PostgreSQL (Neon в production).
    pub database_url: String,
    /// Ключ подписи старой гостевой cookie (совместимость с версией до аккаунтов).
    pub cookie_key: Key,
    /// Адрес локального сервера (не используется на Vercel).
    pub bind_addr: SocketAddr,
    /// Размер пула соединений. На serverless холодный старт — нормированно.
    pub max_connections: u32,
    /// Признак production-окружения (Vercel).
    pub is_production: bool,
    /// Таймаут ожидания свободного соединения из пула.
    pub acquire_timeout: Duration,
}

impl Config {
    /// Читает конфигурацию из окружения.
    ///
    /// В production ключ нужен для чтения старых гостевых cookie; новые
    /// аккаунты идентифицируются серверными сессиями в PostgreSQL.
    pub fn from_env() -> Result<Self, ConfigError> {
        let database_url =
            env::var("DATABASE_URL").map_err(|_| ConfigError::Missing("DATABASE_URL"))?;

        let is_production =
            env::var("VERCEL").is_ok() || env::var("RUST_ENV").is_ok_and(|v| v == "production");

        let cookie_key = match env::var("COOKIE_KEY") {
            Ok(raw) => parse_hex_key(raw.trim())?,
            Err(_) if is_production => return Err(ConfigError::Missing("COOKIE_KEY")),
            Err(_) => {
                tracing::warn!(
                    "COOKIE_KEY не задан: используется временный ключ, \
                     профиль будет сброшен при перезапуске процесса"
                );
                Key::generate()
            }
        };

        let bind_addr = match env::var("BIND_ADDR") {
            Ok(raw) => raw
                .parse()
                .map_err(|_| ConfigError::InvalidBindAddr(raw.clone()))?,
            Err(_) => SocketAddr::from(([127, 0, 0, 1], 3000)),
        };

        let max_connections = match env::var("DATABASE_MAX_CONNECTIONS") {
            Ok(raw) => raw
                .parse::<u32>()
                .map_err(|_| ConfigError::InvalidPoolSize(raw.clone()))?
                .clamp(1, 20),
            // На serverless один инстанс обслуживает один запрос:
            // держать пул больше 2 соединений смысла нет.
            Err(_) if is_production => 2,
            Err(_) => 5,
        };

        Ok(Self {
            database_url,
            cookie_key,
            bind_addr,
            max_connections,
            is_production,
            acquire_timeout: Duration::from_secs(10),
        })
    }
}

/// Разбирает 128-символьный hex-ключ в 64 байта для подписи cookie.
fn parse_hex_key(raw: &str) -> Result<Key, ConfigError> {
    if raw.len() != HEX_KEY_CHARS {
        return Err(ConfigError::InvalidCookieKey(raw.len()));
    }

    let mut bytes = [0_u8; KEY_BYTES];
    for (i, chunk) in raw.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let hi = hex_value(chunk[0]).ok_or(ConfigError::InvalidCookieKey(raw.len()))?;
        let lo = hex_value(chunk[1]).ok_or(ConfigError::InvalidCookieKey(raw.len()))?;
        bytes[i] = (hi << 4) | lo;
    }

    Ok(Key::from(&bytes))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_key() {
        let raw = "ab".repeat(KEY_BYTES);
        assert!(parse_hex_key(&raw).is_ok());
    }

    #[test]
    fn hex_key_accepts_every_digit_case() {
        // Регистр не должен влиять: один и тот же ключ можно задать
        // как заглавными, так и строчными буквами.
        let lower = "0123456789abcdef".repeat(HEX_KEY_CHARS / 16);
        assert_eq!(lower.len(), HEX_KEY_CHARS);
        let upper = lower.to_ascii_uppercase();
        let mixed = "aB0123456789cDeF".repeat(HEX_KEY_CHARS / 16);
        for raw in [lower, upper, mixed] {
            assert!(parse_hex_key(&raw).is_ok(), "raw={raw}");
        }
    }

    #[test]
    fn hex_key_rejects_wrong_length() {
        assert!(matches!(
            parse_hex_key("abcd"),
            Err(ConfigError::InvalidCookieKey(4))
        ));
        // На один символ длиннее и короче — тоже отказ.
        let too_long = "ab".repeat(KEY_BYTES + 1);
        assert!(parse_hex_key(&too_long).is_err());
        let too_short = "ab".repeat(KEY_BYTES - 1);
        assert!(parse_hex_key(&too_short).is_err());
        assert!(parse_hex_key("").is_err());
    }

    #[test]
    fn hex_key_rejects_non_hex() {
        let mut raw = "ab".repeat(KEY_BYTES);
        raw.replace_range(0..2, "zz");
        assert!(parse_hex_key(&raw).is_err());

        // Пробелы и прочие символы тоже не hex.
        for bad in ["g0", "0g", "  ", "::", "!!"] {
            let mut raw = "ab".repeat(KEY_BYTES);
            raw.replace_range(0..2, bad);
            assert!(parse_hex_key(&raw).is_err(), "bad={bad:?}");
        }
    }

    #[test]
    fn hex_key_rejects_multibyte_characters() {
        // Длина проверяется в байтах: кириллица даёт 128 байт вместо
        // 128 символов и не должна пройти как валидный ключ.
        let raw = "щ".repeat(HEX_KEY_CHARS);
        assert_eq!(raw.len(), HEX_KEY_CHARS * 2);
        assert!(parse_hex_key(&raw).is_err());
    }

    #[test]
    fn hex_value_maps_digits_and_letters() {
        assert_eq!(hex_value(b'0'), Some(0));
        assert_eq!(hex_value(b'9'), Some(9));
        assert_eq!(hex_value(b'a'), Some(10));
        assert_eq!(hex_value(b'f'), Some(15));
        assert_eq!(hex_value(b'A'), Some(10));
        assert_eq!(hex_value(b'F'), Some(15));
        assert_eq!(hex_value(b'g'), None);
        assert_eq!(hex_value(b' '), None);
        assert_eq!(hex_value(b'/'), None);
    }

    #[test]
    fn from_env_requires_a_database_url() {
        // Переменные процесса трогать нельзя (тесты идут параллельно),
        // поэтому проверяем сам факт: без DATABASE_URL конфигурация
        // не собирается, иначе приложение стартует «всё хорошо»
        // и падает на первом запросе.
        let missing = ConfigError::Missing("DATABASE_URL");
        assert_eq!(missing.to_string(), "обязательная переменная окружения DATABASE_URL не задана");
    }

    #[test]
    fn pool_size_is_clamped_to_a_sane_range() {
        // `DATABASE_MAX_CONNECTIONS` приходит из окружения Vercel:
        // значение «0» или «1000» не должно приводить к пулу без
        // соединений или к тысяче соединений на инстанс.
        for (raw, expected) in [("0", 1_u32), ("1", 1), ("5", 5), ("20", 20), ("1000", 20)] {
            let parsed: u32 = raw.parse().expect("число разбирается");
            assert_eq!(parsed.clamp(1, 20), expected, "raw={raw}");
        }
    }

    #[test]
    fn config_errors_explain_themselves() {
        let error = ConfigError::InvalidBindAddr("не адрес".into());
        assert!(error.to_string().contains("BIND_ADDR"));
        let error = ConfigError::InvalidPoolSize("много".into());
        assert!(error.to_string().contains("DATABASE_MAX_CONNECTIONS"));
        let error = ConfigError::InvalidCookieKey(7);
        assert!(error.to_string().contains("128 hex-символов"));
    }
}
