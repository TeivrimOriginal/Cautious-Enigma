# LinguaRust Desktop

Нативное десктопное окно для Windows на Rust + WebView2. Внутри работает та же
офлайн-версия сайта: карточки, словарь, чтение, грамматика, экзамен, тренажёр
письма и прогресс в `localStorage`.

## Что умеет оболочка

| Возможность | Как это выглядит для пользователя |
| --- | --- |
| Резервные копии на диске | Прогресс автоматически дублируется в `%APPDATA%\LinguaRust\progress.json` |
| Восстановление | Пустое окно молча поднимает копию с диска; кнопка ♻️ — вручную |
| Экспорт в файл | Кнопка 💾 пишет `exports\linguarust-ГГГГ-ММ-ДД-ЧЧММСС.json` |
| Открыть папку данных | Кнопка 📂 открывает каталог в Проводнике |
| Автозапуск | Кнопка 🚀/🚫 пишет ключ в `HKCU\...\Run` с аргументом `--autostart` |
| Заголовок окна | Тёмный или светлый — как тема в приложении |
| Геометрия окна | Размер, позиция и развёрнутое состояние запоминаются |
| Командная палитра | `Ctrl+K` в окне: переходы по разделам и нативные действия |

## Запуск из исходников

Нужны Rust 1.94+ и WebView2 Runtime (на Windows 11 обычно установлен).

```powershell
cargo run --manifest-path desktop/Cargo.toml
```

## Release-сборка

```powershell
cargo build --release --manifest-path desktop/Cargo.toml
.\desktop\target\release\linguarust-desktop.exe
```

Ключи запуска:

```text
--autostart       открыть окно свёрнутым (так запускает автозапуск Windows)
--data-dir <DIR>  каталог для настроек и резервных копий прогресса
--version         показать версию
--help            показать справку
```

Node.js, Python и внешний сервер не нужны.

## Как устроено

```text
desktop/
├── Cargo.toml
├── src/
│   ├── main.rs      окно, геометрия, тема, цикл событий
│   ├── server.rs    локальный HTTP-сервер и мост /__app/*
│   ├── bridge.js    скрипт window.linguarustDesktop (вставляется в WebView)
│   ├── appdata.rs   настройки и резервные копии в %APPDATA%\LinguaRust
│   ├── autostart.rs ветка реестра HKCU\...\Run
│   ├── cli.rs       разбор аргументов командной строки
│   └── util.rs      даты и время без внешних зависимостей
├── site/            офлайн-копия веб-версии
└── sync-site.ps1    обновление site из web/ и data/
```

При запуске поднимается локальный HTTP-сервер на случайном порту `127.0.0.1`
и в окно грузится `desktop/site/index.html`. Локальный origin нужен, чтобы
работали `fetch` к данным, `localStorage` и service worker: из `file://` Chromium
их блокирует.

### Мост `window.linguarustDesktop`

Скрипт `bridge.js` подставляется в WebView до загрузки страницы, поэтому сайт
видит объект с нативными возможностями. В обычном браузере объекта нет, и
приложение работает как статическая версия.

Каждый запрос к `/__app/*` подписан токеном запуска (`X-LinguaRust-Token`).
Случайный порт localhost доступен любому локальному процессу, а токен живёт
только внутри текущего окна.

| Действие | Запрос | Результат |
| --- | --- | --- |
| `GET /__app/info` | — | версия, каталог данных, состояние автозапуска |
| `POST /__app/backup` | `{payload}` | запись `progress.json` |
| `GET /__app/backup` | — | последняя копия |
| `POST /__app/export` | `{payload}` | файл в `exports\` |
| `POST /__app/autostart` | `{enabled}` | ключ реестра |
| `POST /__app/theme` | `{theme}` | тема заголовка окна |
| `POST /__app/open-data-dir` | — | Проводник с каталогом данных |

Запросы из потоков сервера попадают в окно через `EventLoopProxy`, поэтому
смена темы или автозапуска применяется в UI-потоке.

## Файлы данных

```text
%APPDATA%\LinguaRust\
├── settings.json    размер, позиция окна, тема, автозапуск
├── progress.json    последняя резервная копия прогресса
└── exports\         файлы экспорта с датой в имени
```

`settings.json` и `progress.json` пишутся через временный файл и переименование,
поэтому оборванное закрытие не оставит обрезанный JSON.

## Обновление веб-контента

Если менялись `web/`, `data/` или `static/style.css`, синхронизируй копию
desktop-сайта:

```powershell
.\desktop\sync-site.ps1
```

`desktop/site` намеренно хранится в репозитории: бинарник можно запускать из
`target/release`, не публикуя Node.js-файлы.

## Проверки

```powershell
cargo fmt --manifest-path desktop/Cargo.toml -- --check
cargo clippy --manifest-path desktop/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path desktop/Cargo.toml
```
