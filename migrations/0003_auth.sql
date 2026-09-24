-- Регистрация и вход: логин, хеш пароля и серверные сессии.
--
-- Гостевой режим сохраняется: у гостя username и password_hash = NULL.
-- У зарегистрированного пользователя оба поля заполнены.

ALTER TABLE profiles ADD COLUMN IF NOT EXISTS username TEXT;
ALTER TABLE profiles ADD COLUMN IF NOT EXISTS password_hash TEXT;

-- Логины нечувствительны к регистру и не могут повторяться.
CREATE UNIQUE INDEX IF NOT EXISTS idx_profiles_username
    ON profiles (lower(username))
    WHERE username IS NOT NULL;

-- Сессии: в cookie лежит непрозрачный токен, сам пароль в базе не хранится.
CREATE TABLE IF NOT EXISTS sessions (
    token      UUID PRIMARY KEY,
    profile_id UUID        NOT NULL REFERENCES profiles (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_profile ON sessions (profile_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions (expires_at);
