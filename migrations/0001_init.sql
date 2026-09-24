-- LinguaRust: базовая схема (PostgreSQL)
-- Применяется автоматически при старте приложения (sqlx::migrate!).

-- Профили: без паролей, идентификация по подписанной cookie с UUID.
CREATE TABLE IF NOT EXISTS profiles (
    id          UUID PRIMARY KEY,
    name        TEXT        NOT NULL CHECK (length(btrim(name)) BETWEEN 1 AND 40),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Карточки слов. Поля repetitions / interval_days / ease — состояние SM-2.
CREATE TABLE IF NOT EXISTS cards (
    id                BIGSERIAL PRIMARY KEY,
    profile_id        UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    front             TEXT        NOT NULL CHECK (length(btrim(front)) > 0),
    back              TEXT        NOT NULL CHECK (length(btrim(back)) > 0),
    example           TEXT,
    repetitions       INTEGER     NOT NULL DEFAULT 0,
    interval_days     INTEGER     NOT NULL DEFAULT 0,
    ease              DOUBLE PRECISION NOT NULL DEFAULT 2.5 CHECK (ease >= 1.3),
    due_date          DATE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_reviewed_at  TIMESTAMPTZ,
    CONSTRAINT cards_front_unique_per_profile UNIQUE (profile_id, front)
);

CREATE INDEX IF NOT EXISTS idx_cards_profile_due ON cards (profile_id, due_date);
CREATE INDEX IF NOT EXISTS idx_cards_profile_created ON cards (profile_id, created_at DESC);

-- Журнал повторений: источник данных для стриков и точности.
CREATE TABLE IF NOT EXISTS review_log (
    id           BIGSERIAL PRIMARY KEY,
    card_id      BIGINT      NOT NULL REFERENCES cards (id) ON DELETE CASCADE,
    profile_id   UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    quality      SMALLINT    NOT NULL CHECK (quality BETWEEN 0 AND 5),
    reviewed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_review_log_profile_time
    ON review_log (profile_id, reviewed_at DESC);

-- Тексты для чтения с уровнем сложности.
CREATE TABLE IF NOT EXISTS texts (
    id      BIGSERIAL PRIMARY KEY,
    slug    TEXT        NOT NULL UNIQUE,
    title   TEXT        NOT NULL,
    level   TEXT        NOT NULL,
    summary TEXT,
    content TEXT        NOT NULL,
    word_count INTEGER  NOT NULL DEFAULT 0
);

-- Грамматические упражнения: варианты ответа хранятся в JSONB.
CREATE TABLE IF NOT EXISTS grammar_exercises (
    id             BIGSERIAL PRIMARY KEY,
    topic          TEXT        NOT NULL,
    prompt         TEXT        NOT NULL,
    options        JSONB       NOT NULL,
    correct_index  SMALLINT    NOT NULL CHECK (correct_index >= 0),
    explanation    TEXT
);

-- Переводы по клику в текстах — для статистики чтения.
CREATE TABLE IF NOT EXISTS translation_log (
    id             BIGSERIAL PRIMARY KEY,
    profile_id     UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    word           TEXT        NOT NULL,
    translated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_translation_log_profile_time
    ON translation_log (profile_id, translated_at DESC);
