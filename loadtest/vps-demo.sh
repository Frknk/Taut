#!/usr/bin/env bash
# Las pruebas de la demo contra la VPS (https://taut.lat), con Grafana local en vivo.
#
#   loadtest/vps-demo.sh sostenida   3 min a 50 req/s sobre la página de estado
#   loadtest/vps-demo.sh pico        5 req/s, 1 min a 25 req/s, de vuelta a 5
#   loadtest/vps-demo.sh ambas       una detrás de otra
#   loadtest/vps-demo.sh escalones   1 min por escalón: 5, 10, 20, 30, 40 y 50 req/s
#   loadtest/vps-demo.sh preparar    solo el túnel y Grafana, sin carga (antes de la clase)
#   loadtest/vps-demo.sh cache 0     apaga la caché de status-bff en la VPS (30 la vuelve a encender)
#
# Las tasas se cambian por entorno: RATE=20 loadtest/vps-demo.sh sostenida,
# SPIKE_BASE=5 SPIKE_PEAK=40 loadtest/vps-demo.sh pico, RATES=25,50,100 loadtest/vps-demo.sh escalones.
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
STEP_S="${STEP_S:-60}"
# Prometheus local con la configuración que lee la VPS por el túnel.
OBS_DIR="${OBS_DIR:-$HOME/.cache/taut-vps-obs}"

usage() {
  sed -n '2,12p' "$0" | sed 's/^# \{0,1\}//'
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
  # shellcheck disable=SC2029 # $ttl ya se validó: se expande a propósito en el Mac.
  ssh vps "cd /opt/taut && STATUS_CACHE_TTL=$ttl docker compose up -d --wait status-bff" >/dev/null
  echo "caché de status-bff en la VPS: ${ttl} s ($([ "$ttl" -eq 0 ] && echo apagada || echo encendida))"
}

case "${1:-}" in
  cache) [ $# -eq 2 ] || usage; cache "$2"; exit 0 ;;
  sostenida | pico | ambas | escalones | preparar) [ $# -eq 1 ] || usage ;;
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
  ambas)
    sostenida
    echo "Esperando 30 s a que la VPS se recupere..."
    sleep 30
    pico
    ;;
esac
