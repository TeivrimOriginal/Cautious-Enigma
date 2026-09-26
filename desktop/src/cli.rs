//! Разбор аргументов командной строки.

use std::path::PathBuf;

/// Что делать после разбора аргументов.
#[derive(Debug)]
pub enum Launch {
    Help,
    Version,
    Run(Options),
}

/// Подготовленные параметры запуска.
#[derive(Debug, Default)]
pub struct Options {
    /// Запуск свёрнутым (используется ключом автозапуска).
    pub autostart: bool,
    /// Каталог данных вместо `%APPDATA%\LinguaRust`.
    pub data_dir: Option<PathBuf>,
}

/// Справка по ключам.
pub const HELP: &str = "\
LinguaRust Desktop — десктопное приложение для Windows.

Использование:
  linguarust-desktop [опции]

Опции:
  --autostart       открыть окно свёрнутым (так запускает автозапуск Windows)
  --data-dir <DIR>  каталог для настроек и резервных копий прогресса
  --version         показать версию
  --help            показать эту спправку

Горячие клавиши (работают, когда окно свёрнуто):
  Ctrl+Alt+L        открыть окно и начать повторение
  Ctrl+Alt+S        сохранить резервную копию прогресса
  Ctrl+Alt+P        свернуть или восстановить окно

Внутри окна:
  Ctrl+K            командная палитра
  Файл .json или .csv можно перетащить в окно для импорта

Данные приложения хранятся в %APPDATA%\\LinguaRust:
  settings.json  размер, позиция окна и тема
  progress.json  последняя резервная копия прогресса
  exports\\       файлы экспорта с датой в имени
";

/// Разбирает аргументы, отбрасывая имя программы.
pub fn parse<I, T>(args: I) -> Result<Launch, String>
where
    I: IntoIterator<Item = T>,
    T: Into<String>,
{
    let mut options = Options::default();
    let mut iter = args.into_iter().map(Into::into).peekable();

    while let Some(argument) = iter.next() {
        match argument.as_str() {
            "--autostart" => options.autostart = true,
            "--data-dir" => {
                let value = iter.next().ok_or("после --data-dir нужен путь")?;
                options.data_dir = Some(PathBuf::from(value));
            }
            "--version" | "-V" => return Ok(Launch::Version),
            "--help" | "-h" => return Ok(Launch::Help),
            other => match other.strip_prefix("--data-dir=") {
                Some(value) if !value.is_empty() => {
                    options.data_dir = Some(PathBuf::from(value));
                }
                _ => return Err(format!("неизвестный аргумент: {other}")),
            },
        }
    }

    Ok(Launch::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Options, String> {
        match parse(args.iter().map(|value| value.to_string()))? {
            Launch::Run(options) => Ok(options),
            other => Err(format!("unexpected launch: {other:?}")),
        }
    }

    #[test]
    fn reads_flags() {
        let options = run(&["--autostart", "--data-dir", "C:\\data"]).unwrap();
        assert!(options.autostart);
        assert_eq!(options.data_dir, Some(PathBuf::from("C:\\data")));
    }

    #[test]
    fn reads_inline_data_dir() {
        let options = run(&["--data-dir=C:/tmp/linguarust"]).unwrap();
        assert_eq!(options.data_dir, Some(PathBuf::from("C:/tmp/linguarust")));
    }

    #[test]
    fn defaults_are_empty() {
        let options = run(&[]).unwrap();
        assert!(!options.autostart);
        assert!(options.data_dir.is_none());
    }

    #[test]
    fn rejects_unknown_argument() {
        assert!(run(&["--nope"]).is_err());
    }
}
