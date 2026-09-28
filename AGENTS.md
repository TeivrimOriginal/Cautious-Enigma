# AGENTS.md — LinguaRust

> **Сначала прочитай `D:\PROJECT_FOLDER\AUTOPILOT\DOCTRINE.md` целиком.**
> Это приказ, который действует всегда. Без него работать нельзя.

## Проект

Сервис интервальных повторений на Rust: SM-2, колода карточек, веб-интерфейс.
Крейт `linguarust` v0.1.0, **edition 2024** (осторожно с версией языка).

- `axum 0.8` — HTTP, два бинаря: `linguarust` (сервер) и `axum` (`api/axum.rs`, serverless для Vercel)
- `sqlx 0.9` + **postgres** — база (не SQLite!)
- `askama 0.16` — шаблоны в `templates/`
- `argon2` — хеширование паролей
- `chrono`, `dotenvy`, `thiserror 2`

```
src/          ядро, модели, бизнес-логика SM-2
templates/    askama-шаблоны
api/axum.rs   serverless-обработчик для Vercel
migrations/   SQL-миграции
desktop/      Tauri/нативная обёртка + publish.ps1, sync-site.ps1
mobile/       Android + build-apk.ps1
web/          статика
scripts/      dev.ps1, dev.sh
tests/        integration.rs, routes.rs
.github/workflows/  ci.yml, deploy.yml, desktop-release.yml, pages.yml
```

## Верификация

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File D:\PROJECT_FOLDER\AUTOPILOT\verify.ps1 -Project LinguaRust
```

- `cargo build --offline --all-targets`
- `cargo test --offline` — **на данный момент 14 тестов, все зелёные**
- `cargo clippy --offline --all-targets -- -D warnings`

**Exit 0 — можно идти дальше. Exit ≠ 0 — ты не имеешь права завершиться.**

## Известные долги (проверено 2026-09-28)

- Тестов мало для такого объёма: покрыты auth, карточки, маршруты.
  **Не покрыто:** сам алгоритм SM-2, логика due-очереди, работа с `sqlx`-запросами.
  Это приоритет №1 — SM-2 это ядро продукта, а у него нет тестов.
- Один из тестов называется `health_reports_unavailable_database` — то есть
  тесты **рассчитаны на отсутствие живой БД**. При добавлении тестов, которым
  нужен Postgres, продумай изоляцию (это решит, какая БД в CI).
- Два разных бинаря (обычный и serverless) могут разойтись по логике.
  Общий код держи в `src/`, в `api/axum.rs` — только транспорт.

## Правила проекта

- Собирай **только с `--offline`**.
- Пароли — только `argon2`. Никогда не храни и не логируй plaintext.
- Секреты — только из env (`.env.example` — образец). Никаких ключей в коде.
- `edition = "2024"` — не откатывай на 2021 «чтобы заработало».
- `sqlx` без `query!` макроса или с `prepare` — проверь, что сборка не требует
  живой БД, иначе CI и офлайн-сборка сломаются.
- При изменении схемы БД — добавляй **новую** миграцию, не правь старую.
  `migrations/` применяется по порядку на существующих инсталляциях.
