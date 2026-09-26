//! Нативные диалоги: сохранение копии прогресса и открытие файла.
//!
//! Диалоги показываются в UI-потоке окна, а HTTP-сервер ждёт ответ: слот
//! `DialogSlot` клонируется в событие окна и забирает результат обратно.

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

/// Сколько ждём ответа диалога, пока пользователь выбирает файл.
pub const DIALOG_TIMEOUT: Duration = Duration::from_secs(300);

/// Ответ диалога: путь выбранного файла или `None`, если пользователь отменил.
pub type Answer = Option<PathBuf>;

/// Слот ответа на диалог, который можно передать в событие окна.
#[derive(Clone, Default)]
pub struct DialogSlot {
    answer: Arc<Mutex<Option<Answer>>>,
    ready: Arc<(Mutex<bool>, Condvar)>,
}

impl DialogSlot {
    /// Создаёт пустой слот.
    pub fn new() -> Self {
        Self::default()
    }

    /// Окно отвечает: `Some(path)` — выбранный файл, `None` — отмена.
    pub fn answer(&self, value: Answer) {
        *self.answer.lock().expect("слот диалога") = Some(value);
        let (lock, condvar) = &*self.ready;
        *lock.lock().expect("слот диалога") = true;
        condvar.notify_all();
    }

    /// Ждёт ответа окна; `None` — окно не ответило за отведённое время.
    pub fn wait(&self, timeout: Duration) -> Option<Answer> {
        let (lock, condvar) = &*self.ready;
        let ready = lock.lock().expect("слот диалога");
        let wait = condvar
            .wait_timeout_while(ready, timeout, |ready| !*ready)
            .expect("слот диалога");
        if wait.1.timed_out() {
            return None;
        }
        drop(wait.0);
        self.answer.lock().expect("слот диалога").clone()
    }
}

/// Диалог сохранения копии прогресса с предложенным именем файла.
pub fn save_copy(suggested: &str) -> Answer {
    rfd::FileDialog::new()
        .set_title("Сохранить копию прогресса")
        .set_file_name(suggested)
        .add_filter("Копия прогресса", &["json"])
        .save_file()
}

/// Диалог открытия копии прогресса или CSV со списком карточек.
pub fn open_copy() -> Answer {
    rfd::FileDialog::new()
        .set_title("Открыть копию прогресса")
        .add_filter("Копия прогресса", &["json"])
        .add_filter("Карточки CSV", &["csv"])
        .pick_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_returns_answer() {
        let slot = DialogSlot::new();
        let answer = slot.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            answer.answer(Some(PathBuf::from("C:/копия.json")));
        });
        assert_eq!(
            slot.wait(Duration::from_secs(2)),
            Some(Some(PathBuf::from("C:/копия.json")))
        );
        handle.join().expect("поток ответа");
    }

    #[test]
    fn slot_reports_cancellation() {
        let slot = DialogSlot::new();
        let answer = slot.clone();
        let handle = std::thread::spawn(move || answer.answer(None));
        assert_eq!(slot.wait(Duration::from_secs(2)), Some(None));
        handle.join().expect("поток ответа");
    }

    #[test]
    fn slot_times_out() {
        let slot = DialogSlot::new();
        assert_eq!(slot.wait(Duration::from_millis(50)), None);
    }
}
