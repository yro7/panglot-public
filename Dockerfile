# 1: Dependency recipe 
FROM rust:1.85-bookworm AS planner
RUN cargo install cargo-chef --locked
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# 2: Build 
FROM rust:1.85-bookworm AS builder
RUN cargo install cargo-chef --locked
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl-dev pkg-config gcc g++ \
    && rm -rf /var/lib/apt/lists/*

COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin panglot

# 3: Runtime 
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl3 ca-certificates \
        python3 python3-pip python3-venv \
        curl \
    && rm -rf /var/lib/apt/lists/*
RUN groupadd --gid 1001 panglot \
    && useradd --uid 1001 --gid panglot --shell /bin/false --create-home panglot

WORKDIR /app

# Python sidecar dependencies in a venv.
COPY requirements.txt .
RUN python3 -m venv /app/.venv \
    && /app/.venv/bin/pip install --no-cache-dir -r requirements.txt
ENV PATH="/app/.venv/bin:$PATH"

COPY --from=builder /app/target/release/panglot /app/panglot

COPY config.yml                /app/config.yml
COPY prompts/                  /app/prompts/
COPY scripts/sidecar.py        /app/scripts/sidecar.py
COPY app/static/               /app/app/static/
COPY docker-entrypoint.sh      /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh

RUN mkdir -p /app/output /app/panglot_audio \
    && chown -R panglot:panglot /app

USER panglot
EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/ || exit 1

ENTRYPOINT ["/app/docker-entrypoint.sh"]
CMD ["/app/panglot"]
