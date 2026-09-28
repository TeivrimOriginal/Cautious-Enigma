//! Импорт и экспорт колоды. Формат — совместимый с Anki.
//!
//! Anki описывает раскладку колонок прямо в файле: первые строки начинаются
//! с `#` и объявляют разделитель, служебные колонки и имена полей.
//!
//! ```text
//! #separator:tab
//! #html:false
//! #notetype column 1
//! #deck column 2
//! #tags column 3
//! #columns:Front<TAB>Back
//! Basic<TAB>Default<TAB><TAB>deadline<TAB>срок сдачи
//! ```
//!
//! Помимо родного формата Anki разбирается и с плоским CSV, который
//! выгружает статическая версия сайта (`;`, `,` или таб, без директив).
//!
//! Модуль ничего не знает про базу и HTTP: разбор и сборка — чистые функции,
//! поэтому правила формата покрываются обычными юнит-тестами, а записью
//! занимается `queries::import_cards`.

/// Разделитель колонок.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    Tab,
    Comma,
    Semicolon,
    Pipe,
    Space,
}

impl Separator {
    /// Разделитель из строки директивы `#separator:tab`.
    ///
    /// Неизвестное значение не угадывается: умолчание Anki — таб, и молча
    /// взять «наверное, запятая» опаснее, чем взять официальное умолчание.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "tab" => Some(Self::Tab),
            "comma" | "," => Some(Self::Comma),
            "semicolon" | ";" => Some(Self::Semicolon),
            "pipe" | "|" => Some(Self::Pipe),
            "space" => Some(Self::Space),
            _ => None,
        }
    }

    /// Разделитель, которым разделяются колонки в строке.
    pub fn as_char(self) -> char {
        match self {
            Self::Tab => '\t',
            Self::Comma => ',',
            Self::Semicolon => ';',
            Self::Pipe => '|',
            Self::Space => ' ',
        }
    }

    /// Имя для директивы `#separator:...`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Tab => "tab",
            Self::Comma => "comma",
            Self::Semicolon => "semicolon",
            Self::Pipe => "pipe",
            Self::Space => "space",
        }
    }
}

/// Что удалось разобрать из файла колоды.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deck {
    pub cards: Vec<DeckEntry>,
    /// Строки-директивы, которые не удалось распознать.
    pub unknown_directives: Vec<String>,
    /// Сколько строк файла не попало в разбор (шапка или превышение лимита).
    pub skipped_rows: usize,
}

/// Карточка, разобранная из файла.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeckEntry {
    pub front: String,
    pub back: String,
    pub example: String,
    pub tags: String,
}

/// Итог импорта для показа пользователю.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Строк в файле, которые разобрались в карточки.
    pub parsed: usize,
    /// Строк, отброшенных из-за пустого слова или перевода.
    pub rejected: usize,
}

impl ImportReport {
    /// Есть ли что импортировать.
    pub fn is_empty(&self) -> bool {
        self.parsed == 0
    }
}

/// Максимальное число строк данных из одного файла: 20 000 с запасом
/// перекрывают любую личную колоду, а лимит защищает пул соединений от
/// одной огромной загрузки.
pub const MAX_ROWS: usize = 20_000;

/// Имена колонок, которые считаются переводом.
const BACK_NAMES: &[&str] = &["back", "перевод", "meaning", "translation", "ответ"];
/// Имена колонок, которые считаются примером.
const EXAMPLE_NAMES: &[&str] = &["example", "comment", "пример", "extras"];
/// Имена колонок, которые считаются словом.
const FRONT_NAMES: &[&str] = &["front", "word", "слово", "term", "question"];

/// Разбирает файл колоды: и формат Anki, и плоский CSV проекта.
///
/// Директивы читаются первыми, все до конца файла: в экспорте Anki они идут
/// сверху, но файл, собранный вручную, может положить их и после шапки —
/// и тогда разбор по одной строке ломался бы на первой же записи.
pub fn parse(text: &str) -> Deck {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);

    let mut header = Header::default();
    let mut rows: Vec<&str> = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        if line.trim_start().starts_with('#') {
            header.read_directive(line.trim());
            continue;
        }
        rows.push(line);
    }

    let mut cards = Vec::new();
    let mut skipped = 0_usize;

    // Раскладка колонок известна только после всех директив.
    resolve_columns(&mut header);

    for (index, line) in rows.iter().enumerate() {
        if index >= MAX_ROWS {
            skipped += 1;
            continue;
        }
        // Первая строка может быть шапкой нашего CSV: «word;translation;example».
        if index == 0 && header.looks_like_header(line) {
            skipped += 1;
            continue;
        }

        let separator = header.separator.unwrap_or_else(|| guess_separator(line));
        let fields = split_row(line, separator);
        cards.push(DeckEntry {
            front: field(&fields, header.front_index),
            back: field(&fields, header.back_index),
            example: field(&fields, header.example_index),
            tags: field(&fields, header.tags_index),
        });
    }

    Deck {
        cards,
        unknown_directives: header.unknown,
        skipped_rows: skipped,
    }
}

/// Директивы файла: разделитель и раскладка колонок.
#[derive(Debug, Default)]
struct Header {
    separator: Option<Separator>,
    /// Сколько ведущих колонок занято служебными (notetype, deck, tags).
    /// В Anki их три, поэтому наши поля начинаются с индекса 3.
    service_columns: usize,
    /// Индекс колонки с метками, объявленный директивой. Нумерация в
    /// директивах с единицы, индексы считаются с нуля.
    tags_directive: Option<usize>,
    /// Имена полей из `#columns:`.
    columns: Vec<String>,
    /// Куда встали нужные колонки.
    front_index: usize,
    back_index: usize,
    example_index: usize,
    tags_index: usize,
    unknown: Vec<String>,
}

impl Header {
    /// Разбирает строку, начинающуюся с `#`.
    ///
    /// Anki пишет директивы двумя способами: `#separator:tab` (имя, двоеточие,
    /// значение) и `#notetype column 1` (имя, пробел, номер). Универсально
    /// разбираются оба, иначе половина формата молча ушла бы в «неизвестные».
    fn read_directive(&mut self, line: &str) {
        let body = line.trim_start_matches('#').trim();
        let (name, value) = match body.split_once(':') {
            Some((name, value)) => (name.trim(), value.trim()),
            None => match body.split_once(char::is_whitespace) {
                Some((name, value)) => (name.trim(), value.trim()),
                None => (body, ""),
            },
        };
        let name = name.to_ascii_lowercase();

        match name.as_str() {
            "separator" => match Separator::parse(value) {
                Some(separator) => self.separator = Some(separator),
                None => self.unknown.push(line.to_string()),
            },
            "notetype" | "deck" | "tags" => {
                // `column 1` — первый столбец, то есть индекс 0.
                if let Some(position) = column_number(body)
                    && position > 0
                {
                    self.service_columns = self.service_columns.max(position);
                    if name == "tags" {
                        self.tags_directive = Some(position - 1);
                    }
                }
            }
            "columns" => {
                // Имена колонок разделены тем же разделителем, что и данные.
                let separator = self.separator.unwrap_or(Separator::Tab).as_char();
                self.columns = value
                    .split(separator)
                    .map(|name| name.trim().to_ascii_lowercase())
                    .filter(|name| !name.is_empty())
                    .collect();
            }
            // Служебная директива: на раскладку колонок не влияет.
            "html" => {}
            _ => self.unknown.push(line.to_string()),
        }
    }

    /// Позиция колонки с одним из имён среди объявленных.
    fn position_of(&self, names: &[&str]) -> Option<usize> {
        self.columns
            .iter()
            .position(|column| names.iter().any(|name| *name == column))
    }

    /// Похожа ли строка на шапку таблицы.
    ///
    /// Проверка намеренно узкая. Широкая версия («в строке встречается
    /// слово `перевод` или `пример`») отбрасывала бы настоящие карточки:
    /// перевод слова *перевод* — обычное дело, и карточка пропадала бы молча.
    /// Поэтому шапкой считается только наш собственный экспорт с
    /// вопросительным знаком и строка вида `front;back;...`.
    fn looks_like_header(&self, line: &str) -> bool {
        if !self.columns.is_empty() {
            return false;
        }
        let separator = self.separator.unwrap_or_else(|| guess_separator(line));
        let fields = split_row(line, separator);
        let first = fields.first().map_or("", String::as_str);
        if first.trim_start().starts_with('?') {
            return true;
        }
        if fields.len() < 2 {
            return false;
        }
        let name = |value: &String| value.trim().to_ascii_lowercase();
        FRONT_NAMES.contains(&name(&fields[0]).as_str())
            && BACK_NAMES.contains(&name(&fields[1]).as_str())
    }
}

/// Достраивает раскладку колонок после чтения всех директив.
///
/// Без `#columns:` работает раскладка Anki: сперва служебные колонки, затем
/// слово, перевод и пример. С `#columns:` имена определяют порядок сами.
fn resolve_columns(header: &mut Header) {
    let service = header.service_columns;
    header.front_index = header
        .position_of(FRONT_NAMES)
        .map_or(service, |position| service + position);
    header.back_index = header
        .position_of(BACK_NAMES)
        .map_or(service + 1, |position| service + position);
    header.example_index = header
        .position_of(EXAMPLE_NAMES)
        .map_or(service + 2, |position| service + position);
    // Метки либо объявлены директивой, либо идут сразу за примером.
    header.tags_index = header
        .tags_directive
        .or_else(|| {
            header
                .position_of(&["tags", "теги", "метки"])
                .map(|position| service + position)
        })
        .unwrap_or(header.example_index + 1);
}

/// Номер колонки из директивы вида `#notetype column 3`.
fn column_number(body: &str) -> Option<usize> {
    let digits: String = body
        .rsplit(' ')
        .next()?
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<usize>().ok()
}

/// Угадывает разделитель по строке с данными.
///
/// Нужен для плоского CSV проекта, где директив нет вовсе.
fn guess_separator(line: &str) -> Separator {
    for candidate in [
        Separator::Tab,
        Separator::Semicolon,
        Separator::Comma,
        Separator::Pipe,
    ] {
        if line.contains(candidate.as_char()) {
            return candidate;
        }
    }
    Separator::Tab
}

/// Разбивает строку на поля, уважая кавычки при разделителе-запятой.
fn split_row(line: &str, separator: Separator) -> Vec<String> {
    let delimiter = separator.as_char();
    if delimiter != ',' && delimiter != ';' {
        return line.split(delimiter).map(str::to_string).collect();
    }

    // Разделитель может встречаться и в запятых, и в точках с запятой:
    // экспорт статической версии использует `;`, а внутри полей бывают запятые.
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            // `""` внутри кавычек — экранированная кавычка, а не конец поля.
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            c if (c == delimiter || c == ',' || c == ';') && !in_quotes => {
                fields.push(std::mem::take(&mut current));
            }
            c => current.push(c),
        }
    }
    fields.push(current);
    fields
}

/// Поле по индексу с обрезкой пробелов.
fn field(fields: &[String], index: usize) -> String {
    fields
        .get(index)
        .map_or_else(String::new, |value| value.trim().to_string())
}

/// Отчёт о разборе: сколько строк годится и сколько отброшено.
pub fn report(deck: &Deck) -> ImportReport {
    let parsed = deck
        .cards
        .iter()
        .filter(|card| !card.front.is_empty() && !card.back.is_empty())
        .count();
    ImportReport {
        parsed,
        rejected: deck.cards.len() - parsed + deck.skipped_rows,
    }
}

/// Собирает колоду в формате Anki: с директивами, чтобы файл открывался
/// и в самом Anki, и в нашем собственном импортере.
pub fn export(cards: &[DeckEntry]) -> String {
    let mut out = String::new();
    out.push_str("#separator:tab\n");
    out.push_str("#html:false\n");
    out.push_str("#notetype column 1\n");
    out.push_str("#deck column 2\n");
    out.push_str("#tags column 3\n");
    out.push_str("#columns:Front\tBack\tExample\n");
    for card in cards {
        out.push_str(
            &[
                "Basic",
                "Default",
                &escape_field(&card.tags),
                &escape_field(&card.front),
                &escape_field(&card.back),
                &escape_field(&card.example),
            ]
            .join("\t"),
        );
        out.push('\n');
    }
    out
}

/// Поле для экспорта: табы и переводы строк ломали бы строку.
fn escape_field(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(front: &str, back: &str) -> DeckEntry {
        DeckEntry {
            front: front.to_string(),
            back: back.to_string(),
            example: String::new(),
            tags: String::new(),
        }
    }

    /* ---------------------------------------------------------------- *
     * Разделители
     * ---------------------------------------------------------------- */

    #[test]
    fn separators_are_parsed_from_anki_directives() {
        for (raw, expected) in [
            ("tab", Separator::Tab),
            ("Tab", Separator::Tab),
            ("  comma  ", Separator::Comma),
            ("semicolon", Separator::Semicolon),
            ("pipe", Separator::Pipe),
            ("space", Separator::Space),
        ] {
            assert_eq!(Separator::parse(raw), Some(expected), "raw={raw}");
        }
    }

    #[test]
    fn unknown_separator_is_not_guessed() {
        assert_eq!(Separator::parse("colon"), None);
        assert_eq!(Separator::parse(""), None);
        assert_eq!(Separator::Tab.as_char(), '\t');
        assert_eq!(Separator::Tab.name(), "tab");
    }

    #[test]
    fn tab_separated_file_is_read() {
        let deck = parse("#separator:tab\ndeadline\tсрок сдачи\nprofit\tприбыль\n");
        assert_eq!(deck.cards.len(), 2);
        assert_eq!(deck.cards[0].front, "deadline");
        assert_eq!(deck.cards[0].back, "срок сдачи");
        assert_eq!(deck.cards[1].front, "profit");
    }

    #[test]
    fn plain_csv_of_the_project_is_read() {
        // Так выгружает статическая версия сайта: без директив, `;`.
        let deck = parse("deadline;срок сдачи\nprofit;прибыль\n");
        assert_eq!(deck.cards.len(), 2);
        assert_eq!(deck.cards[0].front, "deadline");
        assert_eq!(deck.cards[0].back, "срок сдачи");
        assert_eq!(deck.cards[1].front, "profit");
    }

    #[test]
    fn csv_header_row_is_not_imported_as_a_card() {
        // Статическая версия пишет шапку `?word;translation;example`.
        let deck = parse("?word;translation;example\ndeadline;срок сдачи;до пятницы\n");
        assert_eq!(deck.cards.len(), 1, "шапка не должна стать карточкой");
        assert_eq!(deck.cards[0].front, "deadline");
        assert_eq!(deck.cards[0].example, "до пятницы");
        assert_eq!(deck.skipped_rows, 1);
    }

    #[test]
    fn separator_directive_wins_over_the_guess() {
        // Первая строка данных без разделителя, а разделитель объявлен выше:
        // угадывать по ней нельзя, нужен разбор по директивам.
        let deck = parse("#separator:tab\n#html:false\ndeadline\tсрок сдачи\n");
        assert_eq!(deck.cards.len(), 1);
        assert_eq!(deck.cards[0].back, "срок сдачи");
    }

    #[test]
    fn directives_below_the_data_still_apply() {
        let deck = parse("deadline\tсрок сдачи\n#separator:tab\nprofit\tприбыль\n");
        assert_eq!(deck.cards.len(), 2);
        assert_eq!(deck.cards[0].back, "срок сдачи", "директивы читаются до данных");
    }

    /* ---------------------------------------------------------------- *
     * Раскладка колонок
     * ---------------------------------------------------------------- */

    #[test]
    fn anki_service_columns_shift_the_fields() {
        // notetype, deck, tags — первые три, дальше наши поля.
        let deck = parse(
            "#separator:tab\n#notetype column 1\n#deck column 2\n#tags column 3\n\
             Basic\tDefault\tдедлайн\tdeadline\tсрок сдачи\tдо пятницы\n",
        );
        assert_eq!(deck.cards.len(), 1);
        assert_eq!(deck.cards[0].front, "deadline");
        assert_eq!(deck.cards[0].back, "срок сдачи");
        assert_eq!(deck.cards[0].example, "до пятницы");
        assert_eq!(deck.cards[0].tags, "дедлайн");
    }

    #[test]
    fn columns_directive_can_swap_front_and_back() {
        let deck = parse(
            "#separator:tab\n#columns:Back\tFront\nсрок сдачи\tdeadline\n",
        );
        assert_eq!(deck.cards[0].front, "deadline");
        assert_eq!(deck.cards[0].back, "срок сдачи");
    }

    #[test]
    fn extra_columns_land_in_example() {
        let deck = parse("deadline;срок сдачи;до пятницы;дедлайн важно\n");
        assert_eq!(deck.cards[0].example, "до пятницы");
        assert_eq!(deck.cards[0].tags, "дедлайн важно");
    }

    /* ---------------------------------------------------------------- *
     * Разбор строк
     * ---------------------------------------------------------------- */

    #[test]
    fn quoted_fields_keep_their_commas() {
        let deck = parse("\"word, with comma\";\"перевод; с точкой\"\n");
        assert_eq!(deck.cards[0].front, "word, with comma");
        assert_eq!(deck.cards[0].back, "перевод; с точкой");
    }

    #[test]
    fn doubled_quotes_are_unescaped() {
        let deck = parse("\"say \"\"hi\"\"\";перевод\n");
        assert_eq!(deck.cards[0].front, "say \"hi\"");
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let deck = parse("  deadline  ;  срок сдачи  \n");
        assert_eq!(deck.cards[0].front, "deadline");
        assert_eq!(deck.cards[0].back, "срок сдачи");
    }

    #[test]
    fn carriage_returns_and_blank_lines_are_skipped() {
        let deck = parse("deadline;срок\r\n\r\n\r\nprofit;прибыль\r\n");
        assert_eq!(deck.cards.len(), 2);
    }

    #[test]
    fn byte_order_mark_does_not_become_part_of_the_first_word() {
        let deck = parse("\u{feff}deadline;срок сдачи\n");
        assert_eq!(deck.cards[0].front, "deadline");
    }

    #[test]
    fn rows_with_a_missing_translation_are_kept_but_reported() {
        let deck = parse("deadline;\nprofit;прибыль\n");
        assert_eq!(deck.cards.len(), 2, "строки не теряются");
        let report = report(&deck);
        assert_eq!(report.parsed, 1);
        assert_eq!(report.rejected, 1);
    }

    #[test]
    fn an_empty_file_gives_an_empty_deck() {
        let deck = parse("");
        assert!(deck.cards.is_empty());
        assert!(report(&deck).is_empty());
        assert_eq!(report(&deck).rejected, 0);
    }

    #[test]
    fn a_file_of_directives_only_gives_an_empty_deck() {
        let deck = parse("#separator:tab\n#html:false\n#notetype column 1\n");
        assert!(deck.cards.is_empty());
    }

    /* ---------------------------------------------------------------- *
     * Директивы
     * ---------------------------------------------------------------- */

    #[test]
    fn known_directives_are_not_reported_as_unknown() {
        let deck = parse(
            "#separator:tab\n#html:true\n#notetype column 1\n#deck column 2\n\
             #tags column 3\n#columns:Front\tBack\n",
        );
        assert!(
            deck.unknown_directives.is_empty(),
            "{:?}",
            deck.unknown_directives
        );
    }

    #[test]
    fn unknown_directives_are_reported_not_ignored_silently() {
        let deck = parse("#separator:tab\n#какой-то-плагин: 1\ndeadline;срок\n");
        assert_eq!(deck.unknown_directives.len(), 1);
        assert!(deck.unknown_directives[0].contains("какой-то-плагин"));
    }

    #[test]
    fn an_unknown_separator_directive_is_reported() {
        let deck = parse("#separator:двоеточие\ndeadline;срок\n");
        assert_eq!(deck.unknown_directives.len(), 1);
    }

    /* ---------------------------------------------------------------- *
     * Границы
     * ---------------------------------------------------------------- */

    #[test]
    fn a_huge_file_is_cut_at_the_limit() {
        let mut text = String::new();
        for index in 0..(MAX_ROWS + 500) {
            text.push_str(&format!("word{index};перевод\n"));
        }
        let deck = parse(&text);
        assert_eq!(deck.cards.len(), MAX_ROWS);
        assert_eq!(deck.skipped_rows, 500, "обрезанные строки не теряются молча");
    }

    #[test]
    fn a_file_of_separators_does_not_produce_cards() {
        let deck = parse(";;;;\n||||\n,,,,\n");
        let report = report(&deck);
        assert_eq!(report.parsed, 0, "мусор не должен становиться карточками");
    }

    #[test]
    fn very_long_fields_are_kept_whole() {
        // Обрезку по длине делает validate_card, а не разбор: иначе файл
        // молча теряет часть слова и импорт выглядит «почти работающим».
        let long = "a".repeat(500);
        let deck = parse(&format!("{long};перевод\n"));
        assert_eq!(deck.cards[0].front.len(), 500);
    }

    /* ---------------------------------------------------------------- *
     * Экспорт
     * ---------------------------------------------------------------- */

    #[test]
    fn export_produces_anki_directives() {
        let text = export(&[entry("deadline", "срок сдачи")]);
        assert!(text.starts_with("#separator:tab\n"));
        assert!(text.contains("#html:false"));
        assert!(text.contains("#notetype column 1"));
        assert!(text.contains("#deck column 2"));
        assert!(text.contains("#tags column 3"));
    }

    #[test]
    fn exported_deck_round_trips() {
        let original = vec![
            DeckEntry {
                front: "deadline".into(),
                back: "срок сдачи".into(),
                example: "до пятницы".into(),
                tags: "дедлайн".into(),
            },
            entry("profit", "прибыль"),
        ];
        let deck = parse(&export(&original));
        assert_eq!(deck.cards.len(), 2);
        assert_eq!(deck.cards[0], original[0]);
        assert_eq!(deck.cards[1], original[1]);
    }

    #[test]
    fn export_does_not_break_rows_with_tabs_or_newlines() {
        let broken = DeckEntry {
            front: "two\tcolumns".into(),
            back: "two\nlines".into(),
            example: "carriage\rreturn".into(),
            tags: String::new(),
        };
        let deck = parse(&export(&[broken]));
        assert_eq!(deck.cards.len(), 1, "перевод строки не должен рвать карточку");
        assert_eq!(deck.cards[0].front, "two columns");
        assert_eq!(deck.cards[0].back, "two lines");
        assert_eq!(deck.cards[0].example, "carriage return");
    }

    #[test]
    fn export_of_an_empty_deck_is_still_valid_anki() {
        let text = export(&[]);
        assert!(text.starts_with("#separator:tab"));
        assert_eq!(parse(&text).cards.len(), 0);
    }

    #[test]
    fn round_trip_survives_every_separator() {
        // Экспорт всегда табовый, но чужие колоды приходят с чем угодно.
        for separator in ['\t', ';', ',', '|'] {
            let line = format!("deadline{separator}срок сдачи{separator}пример");
            let deck = parse(&line);
            assert_eq!(deck.cards.len(), 1, "separator={separator:?}");
            assert_eq!(deck.cards[0].front, "deadline", "separator={separator:?}");
            assert_eq!(deck.cards[0].back, "срок сдачи", "separator={separator:?}");
        }
    }
}
