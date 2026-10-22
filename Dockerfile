# syntax=docker/dockerfile:1
# Imagen de cualquier servicio de Taut; SERVICE elige el binario (api, scheduler, checker,
# incidents, ...). El contexto de build es la raíz del repositorio:
#   docker build --build-arg SERVICE=checker --build-arg GIT_SHA=$(git rev-parse --short HEAD) -t taut-checker:dev .
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
# Dependencias de todo el workspace: la capa es idéntica para todos los servicios y se
# reutiliza mientras no cambien Cargo.toml ni Cargo.lock. SERVICE se declara después para no
# invalidarla.
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
ARG SERVICE=api
RUN cargo build --release --locked -p "taut-${SERVICE}" \
 && cp "target/release/taut-${SERVICE}" /app/taut-service

FROM debian:bookworm-slim AS runtime
ARG GIT_SHA=unknown
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin taut
COPY --from=builder /app/taut-service /usr/local/bin/taut-service
# Valores de producción; compose y Kubernetes los sobreescriben por entorno.
ENV TAUT_APP__ENV=prod \
    TAUT_APP__COMMIT=${GIT_SHA} \
    TAUT_HTTP__ADDR=0.0.0.0:8080 \
    TAUT_TELEMETRY__LOG_FORMAT=json
# glibc sube su umbral de mmap cada vez que libera un bloque grande y desde ahí los guarda en el
# heap sin devolverlos: auth retenía 19 MB por cada hash Argon2id que coincidió con otro (60 de
# 64 MB en la VPS). Con el umbral fijo, todo bloque de más de 128 KB vuelve al sistema al liberarse.
ENV MALLOC_MMAP_THRESHOLD_=131072
USER taut
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/taut-service"]
