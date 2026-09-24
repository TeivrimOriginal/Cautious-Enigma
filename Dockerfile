# --- Сборка ---
FROM rust:1-slim-bookworm AS builder
WORKDIR /app

# Сначала только манифесты: слой с зависимостями кэшируется между сборками.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src api \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > api/axum.rs \
    && echo '' > src/lib.rs \
    && cargo build --release --locked --bin linguarust \
    && rm -rf src api

COPY . .
# Touch, чтобы cargo увидел реальные исходники.
RUN touch src/lib.rs src/main.rs api/axum.rs \
    && cargo build --release --locked --bin linguarust

# --- Запуск ---
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/linguarust /usr/local/bin/linguarust

ENV BIND_ADDR=0.0.0.0:3000 \
    RUST_LOG=linguarust=info,tower_http=info,warn
EXPOSE 3000

# Приватный профиль пользователя — стандартная практика для контейнеров.
RUN useradd --system --create-home --uid 10001 linguarust
USER linguarust

ENTRYPOINT ["/usr/local/bin/linguarust"]
