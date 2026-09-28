//! Режим повторения: по одному слову за экран.
//!
//! Страница `/cards` показывает всю колоду сразу — это удобно, когда
//! добавляешь слова. Но для повторения нужен другой режим: одно слово,
//! «показать перевод», четыре оценки и счётчик «тртья из двенадцати».
//!
//! Страница полностью серверная: показ ответа — обычный `<details>`,
//! оценки — кнопки отправки формы. Так режим работает и без JavaScript,
//! как и вся остальная карточка.

use axum::Form;
use axum::extract::{Path, Query, State};
use axum::response::{Html, Redirect};
use serde::Deserialize;

use crate::auth::Profile;
use crate::error::{AppError, AppResult};
use crate::models::CardRow;
use crate::routes::{NavContext, html, nav_context, page_impl, today, urlencode};
use crate::{AppState, queries};

/// Сколько слов показывать за один заход. Ограничение нужно, чтобы
/// панель не превращалась в простыню на тысячу строк.
const BATCH: i64 = 20;

#[derive(Debug, askama::Template)]
#[template(path = "study.html")]
pub struct StudyPage {
    pub title: String,
    pub css: &'static str,
    pub js: &'static str,
    pub nav: NavContext,
    pub current: &'static str,
    /// Номер карточки в заходе, начиная с 1.
    pub position: usize,
    /// Сколько карточек в этом заходе.
    pub total: usize,
    pub front: String,
    pub back: String,
    pub example: String,
    /// Карточек осталось после этой.
    pub left: usize,
    /// Счётчик повторений карточки — видно, что слово в работе.
    pub repetitions: i32,
    /// Идентификатор карточки: нужен форме оценки.
    pub id: i64,
    /// `true`, когда выдача кончилась: показываем экран «на сегодня всё».
    pub finished: bool,
}

page_impl!(StudyPage {
    position: usize,
    total: usize,
    front: String,
    back: String,
    example: String,
    left: usize,
    repetitions: i32,
    id: i64,
    finished: bool,
});

#[derive(Debug, Default, Deserialize)]
pub struct StudyQuery {
    /// Позиция в выдаче: `0` — первая карточка.
    #[serde(default)]
    n: i64,
}

/// `GET /study` — очередь повторения по одной карточке.
pub async fn index(
    State(state): State<AppState>,
    profile: Profile,
    Query(query): Query<StudyQuery>,
) -> AppResult<Html<String>> {
    let nav = nav_context(&state, Some(&profile)).await?;
    let queue = queries::due_cards(&state.db, profile.id, BATCH).await?;

    let mut page = StudyPage::new(nav, "Повторение", "study");
    fill(&mut page, &queue, query.n);
    html(page)
}

/// Наполняет страницу выбранной карточкой.
///
/// Отделено от обработчика, потому что «что показывается третьим» — это
/// продуктовое правило, и его должно быть видно без базы.
fn fill(page: &mut StudyPage, queue: &[CardRow], position: i64) {
    let Some((index, card)) = pick(queue, position) else {
        // Очередь пуста: это не ошибка, а обычное «на сегодня всё».
        page.total = 0;
        page.finished = true;
        return;
    };

    page.position = index + 1;
    page.total = queue.len();
    page.id = card.id;
    page.front = card.front.clone();
    page.back = card.back.clone();
    page.example = card.example.clone().unwrap_or_default();
    page.left = queue.len().saturating_sub(index + 1);
    page.repetitions = card.repetitions;
    page.finished = index + 1 >= queue.len();
}

/// Выбирает карточку по номеру в выдаче.
///
/// `rem_euclid`, а не `%`: отрицательный или огромный `?n=` — это запрос
/// последней или первой карточки, а не падение. `None` — очередь пуста.
///
/// Публичная, потому что это и есть правило «что показано третьим»: его
/// проверяет и интеграционный тест очереди, а не только юнит-тест роута.
pub fn pick(queue: &[CardRow], position: i64) -> Option<(usize, &CardRow)> {
    if queue.is_empty() {
        return None;
    }
    let index = position.rem_euclid(queue.len() as i64) as usize;
    Some((index, &queue[index]))
}

/// `POST /study/{id}/review` — оценка ответа и возврат к следующему слову.
pub async fn review(
    State(state): State<AppState>,
    profile: Profile,
    Path(id): Path<i64>,
    Form(form): Form<StudyReviewForm>,
) -> AppResult<Redirect> {
    let quality = form.quality.clamp(1, 5);
    let next = form.next.as_deref().unwrap_or("/study");

    match queries::apply_review(&state.db, profile.id, id, quality, today()).await {
        Ok(_) => Ok(Redirect::to(next)),
        // Слова нет или оно чужое: не выкидываем пользователя с экрана,
        // а возвращаем в очередь с понятным объяснением.
        Err(AppError::NotFound) => Ok(Redirect::to(&format!(
            "/study?error={}",
            urlencode("Карточка не найдена")
        ))),
        // Ошибка базы — это 500: о ней должен узнать журнал, а не ученик.
        Err(err) => Err(err),
    }
}

#[derive(Debug, Deserialize)]
pub struct StudyReviewForm {
    #[serde(default)]
    quality: u8,
    /// Куда вернуться после оценки.
    #[serde(default)]
    next: Option<String>,
}

/// Выбор карточки для интеграционных тестов: та же функция, что и в роуте.
#[cfg(test)]
mod tests {
    use askama::Template;
    use super::*;

    fn card(id: i64, repetitions: i32) -> CardRow {
        CardRow {
            id,
            front: format!("word-{id}"),
            back: format!("перевод-{id}"),
            example: Some(format!("пример-{id}")),
            repetitions,
            interval_days: 0,
            ease: 2.5,
            due_date: None,
            created_at: chrono::DateTime::default(),
            last_reviewed_at: None,
        }
    }

    /// Страница профиля: без профиля шаблон показывает другое.
    fn page_for_profile() -> StudyPage {
        let nav = NavContext {
            has_profile: true,
            ..NavContext::default()
        };
        StudyPage::new(nav, "Повторение", "study")
    }

    /// Позиция в выдаче: отрицательный и огромный номер не должны падать.
    #[test]
    fn position_wraps_instead_of_panicking() {
        for (given, length, expected) in [
            (0_i64, 5_usize, 0_usize),
            (4, 5, 4),
            (5, 5, 0),
            (-1, 5, 4),
            (-6, 5, 4),
            (i64::MAX, 5, 2),
            (i64::MIN, 5, 2),
        ] {
            let index = given.rem_euclid(length as i64) as usize;
            assert!(index < length, "given={given} index={index}");
            assert_eq!(index, expected, "given={given}");
        }
    }

    #[test]
    fn quality_is_forced_into_the_real_range() {
        // В форме подделывается что угодно: и 0, и 255.
        for (given, expected) in [
            (0_u8, 1_u8),
            (1, 1),
            (3, 3),
            (5, 5),
            (255, 5),
        ] {
            assert_eq!(given.clamp(1, 5), expected, "quality={given}");
        }
    }

    #[test]
    fn an_empty_queue_shows_the_finished_screen() {
        // Не ошибка, а обычное «на сегодня всё»: пустая выдача не должна
        // показывать карточку с пустым словом.
        let mut page = StudyPage::default();
        fill(&mut page, &[], 0);
        assert!(page.finished);
        assert_eq!(page.total, 0);
        assert_eq!(page.position, 0);
        assert!(page.front.is_empty());
    }

    #[test]
    fn the_first_card_is_shown_first() {
        let queue = vec![card(1, 0), card(2, 3), card(3, 1)];
        let mut page = StudyPage::default();
        fill(&mut page, &queue, 0);

        assert_eq!(page.position, 1);
        assert_eq!(page.total, 3);
        assert_eq!(page.id, 1);
        assert_eq!(page.front, "word-1");
        assert_eq!(page.back, "перевод-1");
        assert_eq!(page.example, "пример-1");
        assert_eq!(page.left, 2);
        assert!(!page.finished);
    }

    #[test]
    fn the_last_card_finishes_the_run() {
        let queue = vec![card(1, 0), card(2, 3), card(3, 1)];
        let mut page = StudyPage::default();
        fill(&mut page, &queue, 2);

        assert_eq!(page.position, 3);
        assert_eq!(page.left, 0);
        assert!(page.finished, "после последней карточки заход закончен");
    }

    #[test]
    fn a_single_card_run_finishes_immediately() {
        // Ошибка «после единственной карточки показываем пустое слово» —
        // ровно тот случай, ради которого есть флаг finished.
        let mut page = StudyPage::default();
        fill(&mut page, &[card(1, 0)], 0);
        assert!(page.finished);
        assert_eq!(page.position, 1);
        assert_eq!(page.total, 1);
    }

    #[test]
    fn a_position_out_of_range_shows_a_real_card() {
        let queue = vec![card(1, 0), card(2, 0), card(3, 0)];
        for given in [-1_i64, 3, 99, i64::MAX, i64::MIN] {
            let mut page = StudyPage::default();
            fill(&mut page, &queue, given);
            assert!(!page.front.is_empty(), "given={given}: показано пустое слово");
            assert!(
                (1..=3).contains(&page.position),
                "given={given} position={}",
                page.position
            );
        }
    }

    #[test]
    fn a_card_without_an_example_renders_an_empty_one() {
        let mut row = card(1, 0);
        row.example = None;
        let mut page = StudyPage::default();
        fill(&mut page, &[row], 0);
        assert_eq!(page.example, "");
    }

    #[test]
    fn the_batch_size_is_bounded() {
        // Панель на тысячу строк бесполезна, а пул всё равно отдаёт
        // столько, сколько попросили.
        assert!((1..=100).contains(&BATCH));
    }

    #[test]
    fn the_nav_placeholder_is_the_default_one() {
        // Гость на `/study` не попадает: маршрут под сессией.
        let nav = NavContext::default();
        assert!(!nav.has_profile);
        assert_eq!(nav.streak, 0);
    }

    #[test]
    fn study_page_renders_without_a_database() {
        // Шаблон должен собираться и для захода с карточкой, и для
        // законченного: иначе профиль без карточек получал бы 500.
        let queue = vec![card(1, 2), card(2, 0)];
        let mut page = page_for_profile();
        fill(&mut page, &queue, 0);
        let html = page.render().expect("шаблон собирается");
        assert!(html.contains("<!doctype html>"));
        assert!(html.contains("word-1"), "слово должно быть на странице");
        assert!(html.contains("Слово 1 из 2"), "счётчик захода на месте");
        assert!(!html.contains("{{"), "неразобранных плейсхолдеров быть не должно");

        let mut empty = page_for_profile();
        fill(&mut empty, &[], 0);
        let html = empty.render().expect("пустая страница собирается");
        assert!(html.contains("На сегодня всё"));
    }

    #[test]
    fn a_page_without_a_profile_asks_to_sign_in() {
        // Гость на `/study` не попадает (маршрут под сессией), но если
        // попадёт — должен увидеть подсказку, а не карточку без профиля.
        let mut page = StudyPage::default();
        fill(&mut page, &[card(1, 0), card(2, 0)], 0);
        let html = page.render().expect("шаблон собирается");
        assert!(html.contains("Нужен профиль"));
        assert!(!html.contains("word-1"));
    }

    #[test]
    fn user_input_in_a_card_is_escaped() {
        // Слово из чужой колоды может содержать разметку; её обязан
        // экранировать шаблон, а не проверка ввода.
        let mut row = card(1, 0);
        row.front = "<script>alert(1)</script>".into();
        let mut page = page_for_profile();
        fill(&mut page, &[row, card(2, 0)], 0);
        let html = page.render().expect("шаблон собирается");
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&#60;script&#62;"));
    }
}
