# taut: recetas de desarrollo. `just` o `just --list` muestra esta ayuda.

set dotenv-load := true
set shell := ["bash", "-euo", "pipefail", "-c"]

web_dir := "web"
# Base de datos desechable para `sqlx-prepare`: recién migrada, igual que la de CI.
prepare_db := "postgres://taut:taut@localhost:5432/taut_prepare"

# Lista las recetas disponibles
default:
    @just --list --unsorted

# Instala herramientas (brew bundle, cargo), hooks de git, dependencias de web/ y crea .env
setup:
    brew bundle --no-upgrade
    command -v sqlx >/dev/null || cargo install sqlx-cli --no-default-features --features postgres,rustls --locked
    command -v cargo-watch >/dev/null || cargo install cargo-watch --locked
    lefthook install
    cd {{web_dir}} && bun install
    cp -n .env.example .env || true
    @echo "Listo. Ahora: just up && just dev (y just web en otra terminal)"

# Levanta postgres, redis, nats y mailpit y espera a que estén sanos
up:
    docker compose up -d --wait

# Levanta también los servicios de taut (construye las imágenes)
up-all:
    GIT_SHA=$(git rev-parse --short HEAD) docker compose --profile app up -d --build --wait

# Apaga los contenedores (los datos quedan en los volúmenes)
down:
    docker compose --profile app down

# Apaga y borra los volúmenes
nuke:
    docker compose --profile app down -v

# Sigue los logs de los contenedores: just logs postgres
logs *args:
    docker compose logs -f {{args}}

# Corre la api recompilando al guardar
dev:
    cargo watch -q -c -x 'run -p taut-api'

# Corre un servicio en su puerto local: just run checker
run svc:
    TAUT_HTTP__ADDR=0.0.0.0:$(just port {{svc}}) cargo run -p taut-{{svc}}

# Corre juntos los servicios de fondo (scheduler, checker, incidents, notifier); Ctrl-C los detiene
workers:
    cargo build -p taut-scheduler -p taut-checker -p taut-incidents -p taut-notifier
    trap 'kill 0' EXIT; \
    for svc in scheduler checker incidents notifier; do \
      TAUT_HTTP__ADDR=0.0.0.0:$(just port $svc) ./target/debug/taut-$svc & \
    done; \
    wait

# Puerto local de cada servicio (el mismo que publica compose)
[private]
port svc:
    @case {{svc}} in \
      api) echo 8080 ;; scheduler) echo 8081 ;; checker) echo 8082 ;; incidents) echo 8083 ;; \
      notifier) echo 8084 ;; \
      *) echo "servicio desconocido: {{svc}}" >&2; exit 1 ;; \
    esac

# Corre el servidor de desarrollo del frontend
web:
    cd {{web_dir}} && bun run dev

# Formatea Rust y web/
fmt:
    cargo fmt --all
    cd {{web_dir}} && bun run fmt

# Verifica el formato sin cambiar nada
fmt-check:
    cargo fmt --all -- --check
    cd {{web_dir}} && bun run fmt:check

# Lints: clippy con warnings como error, oxlint y svelte-check
lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cd {{web_dir}} && bun run lint && bun run check

# Tests de Rust (nextest, necesita `just up`) y del frontend
test:
    cargo nextest run --workspace
    cd {{web_dir}} && bun run test

# Auditoría de dependencias: avisos de seguridad, licencias y duplicados
deny:
    cargo deny check

# Todo lo que corre CI: formato, lints, tests, deny y build del frontend
check: fmt-check lint test deny
    cd {{web_dir}} && bun run build

# Alias de check
ci: check

# Aplica las migraciones pendientes
migrate:
    sqlx migrate run

# Crea una migración nueva: just migrate-add create_monitors
migrate-add name:
    sqlx migrate add {{name}}

# Recrea la base de datos desde cero y aplica las migraciones
db-reset:
    sqlx database drop -y
    sqlx database create
    sqlx migrate run

# Regenera la caché offline de sqlx (.sqlx/) contra una base vacía recién migrada: la inferencia
# de nulabilidad de sqlx usa EXPLAIN y cambia si la base de desarrollo tiene datos.
sqlx-prepare:
    DATABASE_URL={{prepare_db}} sqlx database reset -y
    DATABASE_URL={{prepare_db}} cargo sqlx prepare --workspace

# Construye la imagen de un servicio con el commit actual: just image checker
image svc="api":
    docker build --build-arg SERVICE={{svc}} --build-arg GIT_SHA=$(git rev-parse --short HEAD) -t taut-{{svc}}:dev .

# Pruebas de carga con k6 (llegan en la semana 6)
loadtest:
    @echo "Los escenarios de k6 llegan en la semana 6 (ver loadtest/README.md)."
