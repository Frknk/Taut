#!/usr/bin/env bash
# Las pruebas de la demo contra la VPS (https://taut.lat), con Grafana local en vivo.
#
#   loadtest/vps-demo.sh sostenida   3 min a 50 req/s sobre la página de estado
#   loadtest/vps-demo.sh pico        5 req/s, 1 min a 25 req/s, de vuelta a 5
#   loadtest/vps-demo.sh ambas       una detrás de otra
#   loadtest/vps-demo.sh escalones   1 min por escalón: 5, 10, 20, 30, 40 y 50 req/s
#   loadtest/vps-demo.sh preparar    solo el túnel y Grafana, sin carga (antes de la clase)
#   loadtest/vps-demo.sh cache 0     apaga la caché de status-bff en la VPS (30 la vuelve a encender)
#   loadtest/vps-demo.sh mixta-sembrar  siembra los tenants lt-m-* de la prueba mixta (200 monitores a 60 s)
#   loadtest/vps-demo.sh mixta       prueba mixta en escalones de usuarios (scenarios/mixed.js), ~12 min
#   loadtest/vps-demo.sh mixta-limpiar  borra los tenants lt-m-* para que sus monitores dejen de chequear
#
# Las tasas se cambian por entorno: RATE=20 loadtest/vps-demo.sh sostenida,
# SPIKE_BASE=5 SPIKE_PEAK=40 loadtest/vps-demo.sh pico, RATES=25,50,100 loadtest/vps-demo.sh escalones.
#
# La mixta se ajusta con SCALES=0.5,1,2,3, STEP_MIXED=180 (segundos por escalón) y el resto de variables de mixed.js
# (VIEWERS, PANEL_USERS…), que pasan por MIXED_ARGS: MIXED_ARGS="-e VIEWERS=500".
#
# Requisitos: `ssh vps` configurado, Docker (OrbStack) corriendo y la semilla lt-s-01…05 cargada
# en la VPS. Grafana queda en http://localhost:3000 (filtra por el testid que imprime al final).
set -euo pipefail
cd "$(dirname "$0")/.."

TARGET="${TARGET:-https://taut.lat}"
RATE="${RATE:-50}"
DURATION="${DURATION:-3m}"
SPIKE_BASE="${SPIKE_BASE:-5}"
SPIKE_PEAK="${SPIKE_PEAK:-25}"
RATES="${RATES:-5,10,20,30,40,50}"
SCALES="${SCALES:-0.5,1,2,3}"
# Tamaño de la semilla mixta y destino de sus chequeos: la VPS no deja chequear direcciones
# internas, así que los monitores le piden /v1/ping a la propia VPS por internet.
LT_MIXED_TENANTS="${LT_MIXED_TENANTS:-20}"
LT_MIXED_MONITORS="${LT_MIXED_MONITORS:-10}"
LT_MIXED_DAYS="${LT_MIXED_DAYS:-7}"
MIXED_ECHO_URL="${MIXED_ECHO_URL:-$TARGET/v1/ping}"
STEP_S="${STEP_S:-60}"
# Prometheus local con la configuración que lee la VPS por el túnel.
OBS_DIR="${OBS_DIR:-$HOME/.cache/taut-vps-obs}"

usage() {
  sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
  exit 1
}

# Túnel de métricas: 127.0.0.1:9100 del Mac → métricas de los servicios en la VPS.
tunnel() {
  if curl -fs -o /dev/null --max-time 3 http://127.0.0.1:9100/status-bff; then
    echo "túnel de métricas: ya activo"
    return
  fi
  echo "túnel de métricas: abriendo"
  ssh -f -N -o ServerAliveInterval=15 -o ExitOnForwardFailure=yes \
    -L 127.0.0.1:9100:127.0.0.1:9100 vps
  curl -fs -o /dev/null --max-time 5 http://127.0.0.1:9100/status-bff \
    || { echo "el túnel no responde: ¿está arriba Caddy en la VPS?" >&2; exit 1; }
}

# Prometheus y Grafana locales apuntando a la VPS en vez de al stack local.
observability() {
  mkdir -p "$OBS_DIR"
  {
    cat <<'EOF'
global:
  scrape_interval: 5s
  scrape_timeout: 4s
scrape_configs:
  - job_name: taut
    static_configs:
EOF
    for s in api auth scheduler checker incidents notifier status-bff; do
      printf '      - targets: ["host.docker.internal:9100"]\n        labels: { service: %s, __metrics_path__: /%s }\n' "$s" "$s"
    done
  } > "$OBS_DIR/prometheus.yml"
  cat > "$OBS_DIR/override.yml" <<EOF
services:
  prometheus:
    volumes:
      - $OBS_DIR/prometheus.yml:/etc/prometheus/prometheus.yml:ro
EOF
  docker compose -f compose.yaml -f "$OBS_DIR/override.yml" --profile obs \
    up -d --no-deps prometheus grafana >/dev/null
  echo "Grafana: http://localhost:3000 (tableros «Taut: servicios» y «k6»)"
}

# El servicio k6 de compose fija STATUS_URL, API_URL y AUTH_URL al stack local: hay que
# pasarlas con -e, BASE_URL sola no basta.
run() {
  LT_STATUS_TENANTS=5 LT_STATUS_MONITORS=5 just loadtest status-page \
    -e STATUS_URL="$TARGET" -e API_URL="$TARGET" -e AUTH_URL="$TARGET" "$@"
}

sostenida() {
  echo "== Carga sostenida: 30 s de rampa y $DURATION a $RATE req/s contra $TARGET"
  run -e RATE="$RATE" -e DURATION="$DURATION" || true
}

pico() {
  echo "== Pico: 1 min a $SPIKE_BASE req/s, 1 min a $SPIKE_PEAK, 2 min a $SPIKE_BASE contra $TARGET"
  run -e PROFILE=spike -e SPIKE_BASE="$SPIKE_BASE" -e SPIKE_PEAK="$SPIKE_PEAK" || true
}

escalones() {
  echo "== Escalones de ${STEP_S} s a $RATES req/s contra $TARGET"
  run -e PROFILE=stress -e RATES="$RATES" -e STEP_S="$STEP_S" || true
}

# Segundos de la caché de páginas en la VPS: recrea status-bff con TAUT_STATUS__CACHE_TTL_SECS.
# Un `docker compose up -d` posterior sin la variable la deja en 30, el valor por defecto.
cache() {
  local ttl="$1"
  [[ "$ttl" =~ ^[0-9]+$ ]] || usage
  local out
  # shellcheck disable=SC2029 # $ttl ya se validó: se expande a propósito en el Mac.
  if ! out=$(ssh vps "cd /opt/taut && STATUS_CACHE_TTL=$ttl docker compose up -d --wait status-bff" 2>&1); then
    echo "$out" >&2
    exit 1
  fi
  echo "caché de status-bff en la VPS: ${ttl} s ($([ "$ttl" -eq 0 ] && echo apagada || echo encendida))"
}

# psql en el contenedor de postgres de la VPS, leyendo el SQL por la entrada estándar.
vps_psql() {
  ssh vps "cd /opt/taut && docker compose exec -T postgres psql -U taut -d taut -q -v ON_ERROR_STOP=1 $*"
}

mixta_sembrar() {
  echo "== Semilla mixta en la VPS: $LT_MIXED_TENANTS tenants x $LT_MIXED_MONITORS monitores a 60 s contra $MIXED_ECHO_URL"
  cat loadtest/seed/mixed-clean.sql loadtest/seed/mixed.sql | vps_psql \
    -v mixed_tenants="$LT_MIXED_TENANTS" -v mixed_monitors="$LT_MIXED_MONITORS" \
    -v mixed_days="$LT_MIXED_DAYS" -v "echo_url='$MIXED_ECHO_URL'" -f -
  vps_psql -f - < loadtest/seed/refresh.sql
}

mixta_limpiar() {
  vps_psql -f - < loadtest/seed/mixed-clean.sql
  echo "tenants lt-m-* borrados de la VPS"
}

# CPU y memoria de cada contenedor cada 10 s mientras corre la prueba: Prometheus no tiene
# métricas del host de la VPS.
sample_stats() {
  local out="$1"
  ssh vps 'while :; do date -u +%FT%TZ; docker stats --no-stream --format "{{.Name}} {{.CPUPerc}} {{.MemUsage}}"; sleep 10; done' \
    > "$out" 2>&1 &
  STATS_PID=$!
  trap 'kill "$STATS_PID" 2>/dev/null || true' EXIT
}

mixta() {
  echo "== Mixta: escalones de ${STEP_MIXED:-180} s a ${SCALES}x usuarios contra $TARGET"
  mkdir -p loadtest/results
  local stats
  stats="loadtest/results/vps-stats-$(date -u +%Y%m%dT%H%M%SZ)-mixed.txt"
  sample_stats "$stats"
  # shellcheck disable=SC2086 # MIXED_ARGS lleva varias opciones de k6 a propósito.
  LT_STATUS_TENANTS=5 LT_STATUS_MONITORS=5 just loadtest mixed \
    -e STATUS_URL="$TARGET" -e API_URL="$TARGET" -e AUTH_URL="$TARGET" \
    -e SCALES="$SCALES" -e STEP_S="${STEP_MIXED:-180}" -e LT_MIXED_TENANTS="$LT_MIXED_TENANTS" \
    ${MIXED_ARGS:-} || true
  echo "CPU y memoria de la VPS en $stats"
}

case "${1:-}" in
  cache) [ $# -eq 2 ] || usage; cache "$2"; exit 0 ;;
  mixta-sembrar) [ $# -eq 1 ] || usage; mixta_sembrar; exit 0 ;;
  mixta-limpiar) [ $# -eq 1 ] || usage; mixta_limpiar; exit 0 ;;
  sostenida | pico | ambas | escalones | preparar | mixta) [ $# -eq 1 ] || usage ;;
  *) usage ;;
esac

curl -fs -o /dev/null --max-time 10 "$TARGET/s/lt-s-01" \
  || { echo "$TARGET/s/lt-s-01 no responde: ¿falta la semilla en la VPS?" >&2; exit 1; }
tunnel
observability

case "$1" in
  preparar) echo "Listo: elige el testid en Grafana cuando corras una prueba" ;;
  sostenida) sostenida ;;
  pico) pico ;;
  escalones) escalones ;;
  mixta) mixta ;;
  ambas)
    sostenida
    echo "Esperando 30 s a que la VPS se recupere..."
    sleep 30
    pico
    ;;
esac
