# LinguaRust Desktop

Нативное десктопное окно для Windows на Rust + WebView2. Внутри работает та же
офлайн-версия сайта: карточки, словарь, чтение, грамматика, экзамен, тренажёр
письма и прогресс в `localStorage`.

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

Node.js, Python и внешний сервер не нужны. При запуске приложение само поднимает
локальный HTTP-сервер на случайном порту `127.0.0.1` и открывает в WebView2
страницу `desktop/site/index.html`. Так работают `fetch` к данным, `localStorage`
и офлайн-обновление сайта.

## Обновление веб-контента

Если менялись `web/`, `data/` или `static/style.css`, синхронизируй копию
desktop-сайта:

```powershell
.\desktop\sync-site.ps1
```

`desktop/site` намеренно хранится в репозитории: бинарник можно запускать из
`target/release`, не публикуя Node.js-файлы.

## Структура

```text
desktop/
├── Cargo.toml           зависимости tao/wry
├── src/main.rs          окно и локальный HTTP-сервер
├── site/                офлайн-копия веб-версии
└── sync-site.ps1        обновление site из web/ и data/
```
