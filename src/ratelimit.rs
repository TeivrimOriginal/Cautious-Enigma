//! Ограничение частоты попыток входа и регистрации.
//!
//! Без счётчика злоумышленник перебирает пароли к аккаунту, просто отправляя
//! формы одну за другой: Argon2 делает каждый запрос дорогим, но не невозможным.
//!
//! Реализация — скользящее окно в памяти процесса. Это осознанный компромисс:
//! - на serverless (Vercel) каждый инстанс имеет свой счётчик, поэтому лимит
//!   там срабатывает только против перебора в пределах одного инстанса;
//! - общее ограничение на уровне прокси остаётся правильным решением
//!   для продакшена, но код не должен зависеть от инфраструктуры.
//!
//! Логика чистая и не зависит ни от базы, ни от HTTP, поэтому покрывается
//! обычными юнит-тестами.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Сколько попыток входа разрешено за окно.
pub const LOGIN_ATTEMPTS: usize = 8;
/// Сколько попыток регистрации разрешено за окно.
pub const REGISTER_ATTEMPTS: usize = 5;
/// Размер окна.
pub const WINDOW: Duration = Duration::from_secs(300);

/// Причина отказа: сколько секунд осталось ждать.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Denied {
    pub retry_after: Duration,
}

/// Скользящее окно попыток.
///
/// Счётчики чистятся двумя способами: лениво в [`RateLimiter::allow`]
/// (истёкшее окно того же ключа обнуляется) и целиком в
/// [`RateLimiter::sweep`] — иначе память росла бы из-за одноразовых ключей.
pub struct RateLimiter {
    attempts: Mutex<HashMap<String, (Instant, usize)>>,
    limit: usize,
    window: Duration,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(LOGIN_ATTEMPTS)
    }
}

impl RateLimiter {
    /// Счётчик с лимитом попыток за окно [`WINDOW`].
    pub fn new(limit: usize) -> Self {
        Self {
            attempts: Mutex::new(HashMap::new()),
            limit,
            window: WINDOW,
        }
    }

    /// Регистрирует попытку и отвечает, можно ли её выполнить.
    ///
    /// `now` передаётся явно, чтобы тесты не зависели от системных часов.
    pub fn allow(&self, key: &str, now: Instant) -> Result<(), Denied> {
        self.allow_within(key, now, self.window)
    }

    fn allow_within(&self, key: &str, now: Instant, window: Duration) -> Result<(), Denied> {
        let mut attempts = self.lock();
        let entry = attempts.remove(key);

        let (opened, used) = match entry {
            // Окно истекло — начинаем с чистого листа.
            Some((opened, _)) if now.duration_since(opened) >= window => (now, 0),
            Some(current) => current,
            None => (now, 0),
        };

        let used = used.saturating_add(1);
        if used > self.limit {
            // Не записываем превышение: иначе окно никогда не «отпустило» бы
            // ключ после серии неудач, даже когда время вышло.
            attempts.insert(key.to_string(), (opened, used - 1));
            let left = window.saturating_sub(now.duration_since(opened));
            return Err(Denied {
                retry_after: left.max(Duration::from_secs(1)),
            });
        }

        attempts.insert(key.to_string(), (opened, used));
        Ok(())
    }

    /// Сбрасывает счётчик: успешный вход не должен накапливать лимит.
    pub fn forget(&self, key: &str) {
        self.lock().remove(key);
    }

    /// Убирает все окна, которые истекли. Вызывается раз в несколько минут.
    pub fn sweep(&self, now: Instant) {
        self.lock().retain(|_, (opened, _)| now.duration_since(*opened) < self.window);
    }

    /// Сколько попыток уже израсходовано — для диагностики и тестов.
    pub fn used(&self, key: &str) -> usize {
        self.lock().get(key).map_or(0, |(_, used)| *used)
    }

    /// Длина окна (публично для тестов и документации).
    pub fn window(&self) -> Duration {
        self.window
    }

    /// Лимит попыток за окно.
    pub fn limit(&self) -> usize {
        self.limit
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (Instant, usize)>> {
        // `unwrap` здесь оправдан: счётчик живёт дольше паники, и
        // PoisonError после зависшего в мьютексе потока означает, что
        // приложение всё равно уже не восстановится. Ронять весь процесс
        // из-за этого не нужно.
        self.attempts
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Лимитер с произвольным окном: тестам нужно проверять истечение,
    /// не дожидаясь настоящих пяти минут.
    fn limiter(limit: usize, window: Duration) -> RateLimiter {
        RateLimiter {
            attempts: Mutex::new(HashMap::new()),
            limit,
            window,
        }
    }

    /// Отсчётчик времени, не зависящий от системных часов.
    struct Clock(Instant);

    impl Clock {
        fn start() -> Self {
            Self(Instant::now())
        }

        fn advance(&mut self, by: Duration) -> Instant {
            self.0 += by;
            self.0
        }

        fn now(&self) -> Instant {
            self.0
        }
    }

    #[test]
    fn first_attempt_is_always_allowed() {
        let limiter = RateLimiter::default();
        let clock = Clock::start();
        assert!(limiter.allow("1.2.3.4", clock.now()).is_ok());
    }

    #[test]
    fn attempts_up_to_the_limit_are_allowed() {
        let limiter = limiter(3, WINDOW);
        let clock = Clock::start();
        for attempt in 1..=3 {
            assert!(
                limiter.allow("ip", clock.now()).is_ok(),
                "попытка {attempt} должна проходить"
            );
        }
        assert_eq!(limiter.used("ip"), 3);
    }

    #[test]
    fn attempt_over_the_limit_is_denied() {
        let limiter = limiter(3, WINDOW);
        let clock = Clock::start();
        for _ in 0..3 {
            limiter.allow("ip", clock.now()).expect("в пределах лимита");
        }
        let denied = limiter
            .allow("ip", clock.now())
            .expect_err("четвёртая попытка сверх лимита");
        assert_eq!(denied.retry_after, WINDOW);
    }

    #[test]
    fn keys_are_independent() {
        let limiter = limiter(1, WINDOW);
        let clock = Clock::start();
        assert!(limiter.allow("attacker", clock.now()).is_ok());
        assert!(limiter.allow("victim", clock.now()).is_ok());
        assert!(limiter.allow("attacker", clock.now()).is_err());
        assert!(limiter.allow("victim", clock.now()).is_err());
    }

    #[test]
    fn window_expiry_resets_the_counter() {
        let limiter = limiter(2, WINDOW);
        let mut clock = Clock::start();
        limiter.allow("ip", clock.now()).expect("первая");
        limiter.allow("ip", clock.now()).expect("вторая");
        assert!(limiter.allow("ip", clock.now()).is_err());

        // Ровно на границе окна счётчик обнуляется.
        let later = clock.advance(WINDOW);
        assert!(limiter.allow("ip", later).is_ok());
        assert_eq!(limiter.used("ip"), 1);
    }

    #[test]
    fn window_has_not_expired_one_moment_earlier() {
        let limiter = limiter(1, WINDOW);
        let mut clock = Clock::start();
        limiter.allow("ip", clock.now()).expect("первая");
        let almost = clock.advance(WINDOW - Duration::from_secs(1));
        assert!(limiter.allow("ip", almost).is_err());
    }

    #[test]
    fn denial_time_shrinks_as_the_window_elapses() {
        let limiter = limiter(1, WINDOW);
        let mut clock = Clock::start();
        limiter.allow("ip", clock.now()).expect("первая");
        limiter.allow("ip", clock.now()).expect_err("вторая");

        let later = clock.advance(Duration::from_secs(200));
        let denied = limiter.allow("ip", later).expect_err("третья");
        assert_eq!(denied.retry_after, WINDOW - Duration::from_secs(200));
    }

    #[test]
    fn denial_time_is_never_zero() {
        // Иначе клиент получил бы `Retry-After: 0` и залил бы сервер снова.
        let limiter = limiter(1, Duration::from_secs(10));
        let mut clock = Clock::start();
        limiter.allow("ip", clock.now()).expect("первая");
        // За секунду до истечения окна остатка меньше секунды.
        let later = clock.advance(Duration::from_secs(9));
        let denied = limiter.allow("ip", later).expect_err("вторая");
        assert_eq!(denied.retry_after, Duration::from_secs(1));
    }

    #[test]
    fn repeated_denials_do_not_extend_the_block_forever() {
        let limiter = limiter(1, WINDOW);
        let mut clock = Clock::start();
        limiter.allow("ip", clock.now()).expect("первая");
        for _ in 0..50 {
            assert!(limiter.allow("ip", clock.now()).is_err());
        }
        // Окно истекло — доступ возвращается с первой попытки.
        let later = clock.advance(WINDOW);
        assert!(limiter.allow("ip", later).is_ok());
    }

    #[test]
    fn successful_login_forgets_previous_failures() {
        let limiter = limiter(2, WINDOW);
        let clock = Clock::start();
        limiter.allow("ip", clock.now()).expect("первая");
        limiter.allow("ip", clock.now()).expect("вторая");
        assert!(limiter.allow("ip", clock.now()).is_err());

        limiter.forget("ip");
        assert_eq!(limiter.used("ip"), 0);
        assert!(limiter.allow("ip", clock.now()).is_ok());
    }

    #[test]
    fn sweep_removes_only_expired_keys() {
        let limiter = limiter(1, WINDOW);
        let mut clock = Clock::start();
        limiter.allow("old", clock.now()).expect("old");
        let later = clock.advance(WINDOW / 2);
        limiter.allow("fresh", later).expect("fresh");

        limiter.sweep(later);
        assert_eq!(limiter.used("old"), 1, "свежее окно не трогаем");
        assert_eq!(limiter.used("fresh"), 1);

        let after = clock.advance(WINDOW);
        limiter.sweep(after);
        assert_eq!(limiter.used("old"), 0, "истёкшее окно убрано");
    }

    #[test]
    fn unknown_key_consumes_nothing() {
        let limiter = RateLimiter::default();
        let clock = Clock::start();
        assert!(limiter.allow("a", clock.now()).is_ok());
        assert_eq!(limiter.used("b"), 0);
        assert_eq!(limiter.used("unknown"), 0);
    }

    #[test]
    fn production_limits_match_the_documented_ones() {
        let login = RateLimiter::default();
        assert_eq!(login.limit(), LOGIN_ATTEMPTS);
        assert_eq!(login.window(), WINDOW);

        // Границы значений констант: пароль проверяется Argon2, поэтому
        // несколько попыток за окно достаточно человеку и мало для перебора.
        const {
            assert!(LOGIN_ATTEMPTS <= 10);
            assert!(REGISTER_ATTEMPTS < LOGIN_ATTEMPTS);
        }
    }

    #[test]
    fn limiter_is_usable_from_several_threads() {
        // `RateLimiter` лежит в общем состоянии приложения, поэтому
        // `Sync` — обязательное требование, а не украшение.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RateLimiter>();

        let limiter = std::sync::Arc::new(limiter(1_000, WINDOW));
        let now = Clock::start().now();
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let limiter = limiter.clone();
                std::thread::spawn(move || limiter.allow("ip", now).is_ok())
            })
            .collect();

        let allowed = handles
            .into_iter()
            .map(|handle| handle.join().expect("поток не должен паниковать"))
            .filter(|allowed| *allowed)
            .count();
        assert_eq!(allowed, 8);
        assert_eq!(limiter.used("ip"), 8);
    }
}
