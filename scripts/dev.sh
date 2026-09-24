#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ ! -f .env ]]; then
  cp .env.example .env
  echo "Создан .env из .env.example"
fi

if ! command -v docker >/dev/null 2>&1; then
  echo "Docker не найден. Запусти PostgreSQL отдельно и выполни cargo run." >&2
  exit 1
fi

exec docker compose up --build
