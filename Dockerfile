# syntax=docker/dockerfile:1

# Web page. The Collector embeds web/dist at compile time, so build it first.
FROM node:22-bookworm-slim AS web
WORKDIR /src/web
RUN corepack enable && corepack prepare pnpm@11.5.1 --activate
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# Collector and fake Halogen.
FROM rust:1.98-slim-trixie AS rust
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY fixtures/ fixtures/
COPY --from=web /src/web/dist web/dist
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bins \
 && cp target/release/halogen-dashboard target/release/fake-halogen /usr/local/bin/

FROM debian:trixie-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home-dir /data halogen \
 && mkdir -p /data && chown halogen /data
COPY --from=rust /usr/local/bin/halogen-dashboard /usr/local/bin/fake-halogen /usr/local/bin/
USER halogen
WORKDIR /data
ENV HALOGEN_DASHBOARD_LISTEN=0.0.0.0:8790 \
    HALOGEN_DASHBOARD_DB=/data/halogen-dashboard.sqlite
EXPOSE 8790
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s \
  CMD curl -fsS http://127.0.0.1:8790/healthz || exit 1
ENTRYPOINT ["halogen-dashboard"]
