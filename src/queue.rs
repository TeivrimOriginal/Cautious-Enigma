//! Планировщик повторений: что попадает в сегодняшнюю очередь и в каком
//! порядке показывается.
//!
//! Логика намеренно вынесена из SQL: правило выдачи — это продуктовое
//! решение (кто показывается первым), а не деталь реализации хранилища.
//! База отвечает только за выборку кандидатов по индексу, а порядок и
//! отсечение делает [`take_due_cards`] — одну функцию на сервере, которую
//! можно покрыть обычными юнит-тестами.
//!
//! Порядок выдачи и состав очереди проверяются ещё и в `tests/routes.rs`
//! и `tests/integration.rs`: юнит-тесты защищают формулировки правила,
//! интеграционные — что база отдаёт именно этих карточек.

use chrono::NaiveDate;

use crate::models::CardRow;

/// Сортировочный ключ очереди повторения.
///
/// Сначала самые просроченные (меньшая дата — дольше ждали), затем среди
/// равных дат менее повторённые карточки, и наконец `id` — чтобы выдача
/// была детерминированной и не «прыгала» между запросами.
fn sort_key(card: &CardRow) -> (NaiveDate, i32, i64) {
    (
        card.due_date.unwrap_or(NaiveDate::MAX),
        card.repetitions,
        card.id,
    )
}

/// Признак того, что карточка ждёт повторения сегодня.
///
/// Карточка без назначенной даты в очередь не попадает: у неё ещё не было
/// ни одного повторения, а новые слова проходят через `insert_card`,
/// который сразу ставит дату на сегодня.
pub fn is_due(due: Option<NaiveDate>, today: NaiveDate) -> bool {
    due.is_some_and(|date| date <= today)
}

/// Признак того, что карточка просрочена (ждала строго дольше одного дня).
pub fn is_overdue(due: Option<NaiveDate>, today: NaiveDate) -> bool {
    due.is_some_and(|date| date < today)
}

/// Ставит кандидатов в порядок выдачи. Мутирует на месте, ничего не теряя.
pub fn sort_due_cards(cards: &mut [CardRow]) {
    cards.sort_by_key(sort_key);
}

/// Готовит выдачу на сегодня: сортирует и оставляет не больше `limit` карточек.
///
/// `limit` защищён сверху и снизу: отрицательное или нулевое значение
/// означает «ничего не показывать», а не «показать всё».
pub fn take_due_cards(mut cards: Vec<CardRow>, limit: i64) -> Vec<CardRow> {
    if limit <= 0 {
        return Vec::new();
    }
    sort_due_cards(&mut cards);
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    cards.truncate(limit);
    cards
}

/// Отбирает только то, что ждёт повторения, и готовит выдачу.
///
/// Основной вход для данных, которые уже в памяти (например, для
/// предпросмотра и для тестов): сначала фильтр по дате, затем порядок.
pub fn due_queue(cards: Vec<CardRow>, today: NaiveDate, limit: i64) -> Vec<CardRow> {
    let candidates: Vec<CardRow> = cards
        .into_iter()
        .filter(|card| is_due(card.due_date, today))
        .collect();
    take_due_cards(candidates, limit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;

    fn day(ymd: &str) -> NaiveDate {
        NaiveDate::parse_from_str(ymd, "%Y-%m-%d").expect("корректная дата в тесте")
    }

    fn card(id: i64, due: Option<&str>, repetitions: i32) -> CardRow {
        CardRow {
            id,
            front: format!("word-{id}"),
            back: format!("слово-{id}"),
            example: None,
            repetitions,
            interval_days: 0,
            ease: 2.5,
            due_date: due.map(day),
            created_at: DateTime::default(),
            last_reviewed_at: None,
        }
    }

    fn ids(cards: &[CardRow]) -> Vec<i64> {
        cards.iter().map(|card| card.id).collect()
    }

    /* ---------------------------------------------------------------- *
     * Что считается «должным»
     * ---------------------------------------------------------------- */

    #[test]
    fn today_is_due() {
        assert!(is_due(Some(day("2026-09-24")), day("2026-09-24")));
    }

    #[test]
    fn yesterday_is_due_and_overdue() {
        let today = day("2026-09-24");
        assert!(is_due(Some(day("2026-09-23")), today));
        assert!(is_overdue(Some(day("2026-09-23")), today));
    }

    #[test]
    fn tomorrow_is_not_due() {
        let today = day("2026-09-24");
        assert!(!is_due(Some(day("2026-09-25")), today));
        assert!(!is_overdue(Some(day("2026-09-25")), today));
    }

    #[test]
    fn card_without_a_due_date_is_not_in_the_queue() {
        assert!(!is_due(None, day("2026-09-24")));
        assert!(!is_overdue(None, day("2026-09-24")));
    }

    #[test]
    fn today_is_due_but_not_overdue() {
        let today = day("2026-09-24");
        assert!(is_due(Some(today), today));
        assert!(!is_overdue(Some(today), today));
    }

    /* ---------------------------------------------------------------- *
     * Порядок выдачи
     * ---------------------------------------------------------------- */

    #[test]
    fn most_overdue_card_comes_first() {
        let today = day("2026-09-24");
        let queue = due_queue(
            vec![
                card(1, Some("2026-09-24"), 1),
                card(2, Some("2026-09-20"), 1),
                card(3, Some("2026-09-24"), 1),
            ],
            today,
            10,
        );
        assert_eq!(ids(&queue), vec![2, 1, 3]);
    }

    #[test]
    fn same_due_date_is_ordered_by_repetitions() {
        let today = day("2026-09-24");
        let queue = due_queue(
            vec![
                card(1, Some("2026-09-24"), 5),
                card(2, Some("2026-09-24"), 0),
                card(3, Some("2026-09-24"), 2),
            ],
            today,
            10,
        );
        // Новая карточка (0 повторений) идёт раньше зрелой.
        assert_eq!(ids(&queue), vec![2, 3, 1]);
    }

    #[test]
    fn equal_keys_are_broken_by_id_so_the_queue_is_stable() {
        let today = day("2026-09-24");
        let queue = due_queue(
            vec![
                card(9, Some("2026-09-24"), 1),
                card(4, Some("2026-09-24"), 1),
                card(7, Some("2026-09-24"), 1),
            ],
            today,
            10,
        );
        assert_eq!(ids(&queue), vec![4, 7, 9]);
    }

    #[test]
    fn due_date_wins_over_repetitions() {
        let today = day("2026-09-24");
        let queue = due_queue(
            vec![
                card(1, Some("2026-09-24"), 9),
                card(2, Some("2026-09-23"), 0),
            ],
            today,
            10,
        );
        // Просроченная «новая» карточка важнее сегодняшней зрелой.
        assert_eq!(ids(&queue), vec![2, 1]);
    }

    #[test]
    fn sorting_keeps_every_card() {
        let input = vec![
            card(1, Some("2026-09-24"), 0),
            card(2, Some("2026-09-22"), 3),
            card(3, None, 1),
            card(4, Some("2026-09-24"), 1),
        ];
        let mut sorted = input.clone();
        sort_due_cards(&mut sorted);
        assert_eq!(sorted.len(), input.len());
        let before: std::collections::HashSet<i64> = input.iter().map(|c| c.id).collect();
        let after: std::collections::HashSet<i64> = sorted.iter().map(|c| c.id).collect();
        assert_eq!(before, after, "сортировка не должна терять карточки");
    }

    #[test]
    fn sorting_puts_cards_without_a_due_date_last() {
        // Карточка без даты в выдачу не попадает, но если она всё же
        // оказалась в кандидатах — обязана встать последней, а не первой.
        let mut cards = vec![card(1, None, 0), card(2, Some("2026-09-24"), 5)];
        sort_due_cards(&mut cards);
        assert_eq!(ids(&cards), vec![2, 1]);
    }

    /* ---------------------------------------------------------------- *
     * Отсечение
     * ---------------------------------------------------------------- */

    #[test]
    fn limit_keeps_the_first_cards_of_the_queue() {
        let today = day("2026-09-24");
        let queue = due_queue(
            vec![
                card(1, Some("2026-09-24"), 0),
                card(2, Some("2026-09-21"), 0),
                card(3, Some("2026-09-23"), 0),
            ],
            today,
            2,
        );
        assert_eq!(ids(&queue), vec![2, 3]);
    }

    #[test]
    fn limit_larger_than_the_queue_is_harmless() {
        let today = day("2026-09-24");
        let queue = due_queue(vec![card(1, Some("2026-09-24"), 0)], today, 50);
        assert_eq!(ids(&queue), vec![1]);
    }

    #[test]
    fn zero_and_negative_limits_show_nothing() {
        let today = day("2026-09-24");
        let cards = vec![card(1, Some("2026-09-24"), 0), card(2, Some("2026-09-20"), 0)];
        assert!(due_queue(cards.clone(), today, 0).is_empty());
        assert!(due_queue(cards.clone(), today, -5).is_empty());
    }

    #[test]
    fn absurd_limit_does_not_overflow() {
        let today = day("2026-09-24");
        let queue = due_queue(vec![card(1, Some("2026-09-24"), 0)], today, i64::MAX);
        assert_eq!(ids(&queue), vec![1]);
    }

    /* ---------------------------------------------------------------- *
     * Пустая очередь
     * ---------------------------------------------------------------- */

    #[test]
    fn the_card_page_and_the_queue_agree_on_what_is_due() {
        // `CardView::new` подсвечивает карточку тегом `due`, а
        // `due_cards` формирует выдачу. Если признаки разойдутся,
        // пользователь увидит «к повторению» карточку, которой в очереди нет.
        use crate::routes::cards::CardView;

        let today = day("2026-09-24");
        let candidates = vec![
            card(1, Some("2026-09-24"), 0),
            card(2, Some("2026-09-20"), 2),
            card(3, Some("2026-09-25"), 0),
            card(4, None, 0),
        ];

        let queue = due_queue(candidates.clone(), today, 10);
        let due_ids: std::collections::HashSet<i64> = queue.iter().map(|c| c.id).collect();

        for row in &candidates {
            let view = CardView::new(row, today);
            assert_eq!(
                view.is_due,
                due_ids.contains(&row.id),
                "карточка {}: тег «к повторению» и очередь разошлись",
                row.id
            );
        }
    }

    #[test]
    fn empty_input_gives_empty_queue() {
        assert!(due_queue(Vec::new(), day("2026-09-24"), 10).is_empty());
    }

    #[test]
    fn nothing_due_gives_empty_queue() {
        let today = day("2026-09-24");
        let queue = due_queue(
            vec![
                card(1, Some("2026-10-01"), 0),
                card(2, None, 0),
            ],
            today,
            10,
        );
        assert!(queue.is_empty(), "завтрашние и не назначенные не показываем");
    }

    #[test]
    fn queue_mixed_with_future_cards() {
        let today = day("2026-09-24");
        let queue = due_queue(
            vec![
                card(1, Some("2026-10-05"), 0),
                card(2, Some("2026-09-24"), 0),
                card(3, Some("2026-09-20"), 4),
                card(4, None, 0),
            ],
            today,
            10,
        );
        assert_eq!(ids(&queue), vec![3, 2]);
    }
}
