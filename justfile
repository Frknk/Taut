# taut: recetas de desarrollo. `just` o `just --list` muestra esta ayuda.

set dotenv-load := true
set shell := ["bash", "-euo", "pipefail", "-c"]

web_dir := "web"
# Base de datos desechable para `sqlx-prepare`: recién migrada, igual que la de CI.
prepare_db := "postgres://taut:taut@localhost:5432/taut_prepare"
# Todos los perfiles de compose, para que `down` y `nuke` no dejen contenedores sueltos.
all_profiles := "--profile app --profile obs --profile loadtest"
# psql dentro del contenedor de postgres (no hace falta instalarlo en la máquina).
psql := "docker compose exec -T postgres psql -U taut -d taut -q -v ON_ERROR_STOP=1"
# Tamaños de la semilla de carga; `loadtest-seed` y `loadtest` usan los mismos (LT_* los cambia).
lt_sizes := "-v status_tenants=" + env("LT_STATUS_TENANTS", "20") + " -v status_monitors=" + env("LT_STATUS_MONITORS", "10") + " -v status_days=" + env("LT_STATUS_DAYS", "89") + " -v status_step_min=" + env("LT_STATUS_STEP_MIN", "5") + " -v panel_tenants=" + env("LT_PANEL_TENANTS", "50") + " -v panel_monitors=" + env("LT_PANEL_MONITORS", "5") + " -v panel_days=" + env("LT_PANEL_DAYS", "30") + " -v panel_step_min=" + env("LT_PANEL_STEP_MIN", "1")
lt_k6_env := "-e LT_STATUS_TENANTS=" + env("LT_STATUS_TENANTS", "20") + " -e LT_STATUS_MONITORS=" + env("LT_STATUS_MONITORS", "10") + " -e LT_PANEL_TENANTS=" + env("LT_PANEL_TENANTS", "50") + " -e LT_PANEL_MONITORS=" + env("LT_PANEL_MONITORS", "5")

# Lista las recetas disponibles
default:
    @just --list --unsorted

# Borra target/ (artefactos de compilación viejos que se acumulan con el tiempo)
clean:
    cargo clean

# Instala herramientas (brew bundle, cargo), hooks de git, dependencias de web/ y crea .env
setup:
    brew bundle --no-upgrade
    command -v sqlx >/dev/null || cargo install sqlx-cli --no-default-features --features postgres,rustls --locked
    command -v cargo-watch >/dev/null || cargo install cargo-watch --locked
    lefthook install
    cd {{web_dir}} && bun install
    # Los tests de componentes de web/ corren en Chromium
    cd {{web_dir}} && bunx playwright install chromium
    cp -n .env.example .env || true
    @echo "Listo. Ahora: just up && just dev (y just web en otra terminal)"

# Levanta postgres, redis, nats y mailpit y espera a que estén sanos
up:
    docker compose up -d --wait

# Levanta también los servicios de Taut (construye las imágenes)
up-all:
    GIT_SHA=$(git rev-parse --short HEAD) docker compose --profile app up -d --build --wait

# Apaga los contenedores (los datos quedan en los volúmenes)
down:
    docker compose {{all_profiles}} down

# Apaga y borra los volúmenes
nuke:
    docker compose {{all_profiles}} down -v

# Levanta Prometheus (http://localhost:9090) y Grafana (http://localhost:3000)
obs-up:
    docker compose --profile obs up -d --wait

# Sigue los logs de los contenedores: just logs postgres
logs *args:
    docker compose logs -f {{args}}

# Corre la api recompilando al guardar
dev:
    cargo watch -q -c -x 'run -p taut-api'

# Corre un servicio en su puerto local: just run checker
run svc:
    TAUT_HTTP__ADDR=0.0.0.0:$(just port {{svc}}) cargo run -p taut-{{svc}}

# Corre juntos los servicios que no son la api (auth, scheduler, checker, incidents, notifier y
# status-bff); Ctrl-C los detiene
workers:
    cargo build -p taut-auth -p taut-scheduler -p taut-checker -p taut-incidents -p taut-notifier -p taut-status-bff
    target_dir=$(cargo metadata --format-version 1 --no-deps | jq -r .target_directory); \
    trap 'kill 0' EXIT; \
    for svc in auth scheduler checker incidents notifier status-bff; do \
      TAUT_HTTP__ADDR=0.0.0.0:$(just port $svc) "$target_dir/debug/taut-$svc" & \
    done; \
    wait

# Puerto local de cada servicio (el mismo que publica compose)
[private]
port svc:
    @case {{svc}} in \
      api) echo 8080 ;; scheduler) echo 8081 ;; checker) echo 8082 ;; incidents) echo 8083 ;; \
      notifier) echo 8084 ;; status-bff) echo 8085 ;; auth) echo 8086 ;; \
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

# Pruebas de carga (loadtest/README.md): servicios release, observabilidad y el servidor eco
loadtest-up:
    GIT_SHA=$(git rev-parse --short HEAD) docker compose --profile app --profile obs --profile loadtest up -d --build --wait

# Borra los datos de carga anteriores y siembra los nuevos (tarda unos minutos): just loadtest-seed
loadtest-seed set="base": loadtest-clean
    {{psql}} {{lt_sizes}} -v echo_url="${ECHO_URL:-http://echo/}" -f - < loadtest/seed/{{set}}.sql
    {{psql}} -f - < loadtest/seed/refresh.sql

# Borra todo lo que creó la semilla de carga (slugs lt-*, correos @loadtest.taut.test)
loadtest-clean:
    {{psql}} -f - < loadtest/seed/clean.sql
    {{psql}} -f - < loadtest/seed/refresh.sql

# Corre un escenario de k6; lo que sigue va a k6: just loadtest status-page -e SLUGS=hot
loadtest scenario *args:
    #!/usr/bin/env bash
    set -euo pipefail
    testid="$(date -u +%Y%m%dT%H%M%SZ)-{{scenario}}"
    mkdir -p "loadtest/results/$testid"
    docker compose --profile k6 run --rm \
      -e K6_WEB_DASHBOARD=true -e K6_WEB_DASHBOARD_EXPORT="results/$testid/report.html" \
      k6 run --out experimental-prometheus-rw \
      --tag testid="$testid" --tag commit="$(git rev-parse --short HEAD)" \
      --summary-trend-stats "avg,min,med,p(90),p(95),p(99),max" \
      --summary-export "results/$testid/summary.json" \
      {{lt_k6_env}} {{args}} "scenarios/{{scenario}}.js"
    echo "Resultados en loadtest/results/$testid (testid=$testid en Grafana)"

# Guarda el resumen de una corrida para el reporte, sin setup_data (lleva los tokens del login)
loadtest-keep testid:
    mkdir -p docs/loadtest/data
    jq 'del(.setup_data)' "loadtest/results/{{testid}}/summary.json" > "docs/loadtest/data/{{testid}}.json"

# Revisa los escenarios de k6 sin correrlos: oxlint, oxfmt (estilo de web/) y `k6 inspect`
loadtest-lint:
    {{web_dir}}/node_modules/.bin/oxlint loadtest
    {{web_dir}}/node_modules/.bin/oxfmt -c {{web_dir}}/.oxfmtrc.json --check loadtest
    @for f in loadtest/scenarios/*.js; do docker compose --profile k6 run --rm k6 inspect "scenarios/$(basename "$f")" > /dev/null && echo "ok $f"; done
