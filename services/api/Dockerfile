# syntax=docker/dockerfile:1
# Imagen de taut-api. El contexto de build es la raíz del repositorio:
#   docker build -f services/api/Dockerfile --build-arg GIT_SHA=$(git rev-parse --short HEAD) -t taut-api:dev .
# Tres etapas: cargo-chef cachea la compilación de dependencias, builder compila el
# binario y runtime es Debian slim sin toolchain.

ARG CHEF_IMAGE=lukemathwalker/cargo-chef:latest-rust-1.98.1-slim-bookworm

FROM ${CHEF_IMAGE} AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Solo dependencias: esta capa se reutiliza mientras no cambien Cargo.toml ni Cargo.lock.
RUN cargo chef cook --release --recipe-path recipe.json -p taut-api
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release --locked -p taut-api

FROM debian:bookworm-slim AS runtime
ARG GIT_SHA=unknown
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin taut
COPY --from=builder /app/target/release/taut-api /usr/local/bin/taut-api
# Valores de producción; compose y Kubernetes los sobreescriben por entorno.
ENV TAUT_APP__ENV=prod \
    TAUT_APP__COMMIT=${GIT_SHA} \
    TAUT_HTTP__ADDR=0.0.0.0:8080 \
    TAUT_TELEMETRY__LOG_FORMAT=json
USER taut
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/taut-api"]
