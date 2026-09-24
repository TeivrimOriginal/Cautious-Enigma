-- Игровые механики: дневная цель и журнал грамматики.

-- Сколько повторений в день считать целью (10 / 20 / 50).
ALTER TABLE profiles ADD COLUMN IF NOT EXISTS daily_goal SMALLINT NOT NULL DEFAULT 20;

-- Ответы на упражнения: из них считается опыт и достижения.
CREATE TABLE IF NOT EXISTS grammar_log (
    id           BIGSERIAL PRIMARY KEY,
    profile_id   UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    exercise_id  BIGINT      NOT NULL,
    correct      BOOLEAN     NOT NULL,
    answered_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_grammar_log_profile_time
    ON grammar_log (profile_id, answered_at DESC);
