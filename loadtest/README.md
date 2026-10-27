# loadtest

Pruebas de carga de Taut con [k6](https://grafana.com/docs/k6/), versionadas junto al código. Las
decisiones están en [ADR 0011](../docs/adr/0011-pruebas-de-carga-k6.md) y los reportes de cada
corrida en [docs/loadtest](../docs/loadtest/README.md).

## Requisitos

Solo Docker (OrbStack): k6, psql, Prometheus y Grafana corren en contenedores de `compose.yaml`.
El k6 del `Brewfile` sirve para correr los mismos scripts contra staging.

## Correr una prueba

```sh
just loadtest-up      # servicios release + Prometheus + Grafana + servidor eco
just loadtest-seed    # datos de prueba (unos minutos; borra los anteriores)
just loadtest smoke   # siempre antes de una corrida pesada
just loadtest api-crud
just loadtest status-page
just loadtest-keep <testid>  # guarda el resumen en docs/loadtest/data (sin los tokens)
just loadtest-clean   # al terminar, si quieres liberar la base
```

Cada corrida lleva un `testid` (fecha UTC + escenario) y deja en `loadtest/results/<testid>/` el
`summary.json` y el `report.html` del dashboard web de k6 (la carpeta no se versiona). Las métricas
van a Prometheus en vivo: en Grafana (http://localhost:3000) el tablero **k6 Prometheus (Native
Histograms)** filtra por `testid`, y **Taut: servicios** muestra el lado servidor (latencia por
ruta, conexiones de PostgreSQL, lag de los chequeos).

Lo que va después de `just loadtest <escenario>` pasa a k6, por ejemplo
`just loadtest status-page -e SLUGS=hot -e RATE=1000`.

## Escenarios

| Escenario      | Qué mide                                                                                                                       | Carga por defecto                                                                                                                                                                                         | Umbrales                                                                                                                                       |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `smoke`        | Que el stack, la semilla y los scripts funcionan                                                                               | 1 VU, 3 iteraciones                                                                                                                                                                                       | todos los checks y ninguna petición fallida                                                                                                    |
| `api-crud`     | El panel: lista, detalle, checks, métricas e incidentes, más crear, editar, pausar y borrar                                    | 1 min de calentamiento a 5 it/s; 5 min a 25 it/s de lectura y 1 it/s de escritura                                                                                                                         | p95 < 300 ms (global y por ruta), p99 < 1 s, errores < 1 %, cero 429                                                                           |
| `status-page`  | Páginas de estado públicas, con la caché de Redis (`TAUT_STATUS__CACHE_TTL_SECS=0` la apaga)                                   | 30 s de rampa y 5 min a 500 req/s                                                                                                                                                                         | p95 < 300 ms, p99 < 1 s, errores < 1 %                                                                                                         |
| `rate-limit`   | El presupuesto por tenant: 429 con `Retry-After` al agotarlo, sin afectar a otros                                              | 15 req/s de panel y 2 req/s de API key sobre `lt-rl-a`, 2 req/s de control; 3 ventanas de 1 min                                                                                                           | 600 (panel) y 60 (API key) OK por ventana ±2, `Retry-After` correcto, cero 429 en el control, cero errores del limitador                       |
| `isolation`    | Que ningún tenant llegue a datos de otro bajo carga (RLS, ADR 0010)                                                            | 20 it/s de ataque (14 caminos en rueda) y 8 it/s de uso normal de las víctimas, 3 min                                                                                                                     | cero fugas (aborta en la primera) y cero respuestas inesperadas                                                                                |
| `checker-soak` | El pipeline de chequeos (scheduler, NATS, checker, incidents) con muchos monitores durante mucho tiempo                        | Ninguna petición a Taut: los 2 000 monitores de `just loadtest-seed soak` son la carga; k6 lee Prometheus cada 15 s durante 30 min                                                                        | 99,5 % de los chequeos con lag < 10 s, cero expirados, programados ≈ esperados, completados y guardados ≥ 99 %, cola de NATS < 10 s de trabajo |
| `mixed`        | Todo a la vez en un servidor chico: páginas de estado, primeras visitas con sus archivos, panel, logins, heartbeats y chequeos | Escalones de 3 min de 0,5×, 1×, 2× y 3× una unidad de usuarios (1 000 pestañas de estado, 60 visitas nuevas/min, 20 usuarios de panel, 2 logins/min, 20 heartbeats/min) con 200 monitores a 60 s de fondo | Por escalón: p95 < 300 ms y errores < 1 % en página de estado y panel; errores < 1 % en lo demás; 99,5 % de los chequeos con lag < 10 s        |

`status-page` tiene además tres perfiles (`-e PROFILE=…`) que buscan dónde y cómo se degrada la
página. Sus umbrales dicen qué escalón o fase cumplió el SLO, así que un umbral fallido ahí es un
resultado y no una prueba rota:

| Perfil       | Carga                                                                              | Qué responde                            |
| ------------ | ---------------------------------------------------------------------------------- | --------------------------------------- |
| `baseline`   | El de la tabla de arriba (por defecto)                                             | ¿Cumple el SLO a la carga normal?       |
| `stress`     | Escalones de 2 min: 100, 250, 500, 750, 1 000, 1 250 y 1 500 req/s (`RATES`)       | ¿En qué escalón deja de cumplir el SLO? |
| `spike`      | 1 min a 50 req/s, 1 min a 2 000, 2 min a 50 (`SPIKE_BASE`, `SPIKE_PEAK`)           | ¿Se recupera sola después del pico?     |
| `breakpoint` | Rampa de 0 a 3 000 req/s en 15 min; se corta con p95 > 1 s o más de 5 % de errores | ¿Cuál es la tasa máxima sostenible?     |

`checker-soak` necesita su propio set de datos (`just loadtest-seed soak`, que borra el base) y conviene correrlo con `caffeinate -i`.

`mixed` cuenta personas, no peticiones: cada tipo de tráfico copia lo que hace el navegador (la
página de estado se consulta cada 30 s; el panel pide la lista o el detalle cada 10 s, el contador
de incidentes cada 30 s y las métricas cada 60 s). Está hecho para la VPS de la demo:
`loadtest/vps-demo.sh mixta-sembrar`, `caffeinate -i loadtest/vps-demo.sh mixta` y, al terminar,
`loadtest/vps-demo.sh mixta-limpiar`, porque los monitores siguen chequeando hasta que se borran.
Los logins envían el token falso de Turnstile (`CAPTCHA_TOKEN`): en local auth lo ignora y en la
VPS `mixta` pone a auth con la clave de prueba de Cloudflare mientras corre y la devuelve al
terminar (a mano: `loadtest/vps-demo.sh captcha prueba` y `captcha real`).
Como k6 no tiene caché HTTP, cada visita nueva descarga todos los archivos que precarga el HTML;
los trozos que la app importa después no se piden, así que ese tráfico es un mínimo.

`isolation -e SELFTEST=1` apunta cada atacante a su propio tenant: tiene que fallar, y prueba
que el detector de fugas funciona.

Los escenarios usan un **modelo abierto** (`constant-arrival-rate`): k6 lanza un número fijo de
iteraciones por segundo, haya respondido o no el sistema. Si Taut se pone lento la carga no baja:
sube la latencia y aparecen `dropped_iterations`, que también es un umbral. Los umbrales solo miran
la fase `steady`, no el calentamiento.

Variables de los escenarios:

| Variable                                                    | Escenario                        | Por defecto              | Para qué                                                                                                      |
| ----------------------------------------------------------- | -------------------------------- | ------------------------ | ------------------------------------------------------------------------------------------------------------- |
| `READ_RATE`, `WRITE_RATE`                                   | api-crud                         | 25, 1                    | Iteraciones por segundo; el script se niega a pasar del 80 % del presupuesto de 600 peticiones/min por tenant |
| `RATE`                                                      | status-page                      | 500                      | Peticiones por segundo en la fase estable                                                                     |
| `SLUGS`                                                     | status-page                      | `spread`                 | `spread` reparte entre las 20 páginas; `hot` las manda todas a una                                            |
| `ACCEPT_ENCODING`                                           | status-page                      | `gzip, deflate, br`      | Lo que acepta el navegador del visitante; vacío pide la página sin comprimir                                  |
| `PROFILE`, `STEP_S`, `MAX_VUS`                              | status-page                      | `baseline`, 120, 2000    | Perfil de carga, duración de cada escalón de `stress` y VUs máximos de k6                                     |
| `RATES`                                                     | status-page (`stress`)           | 100,250,…,1500           | Escalones en req/s, separados por coma; `10,25,50` para un servidor chico                                     |
| `SPIKE_BASE`, `SPIKE_PEAK`                                  | status-page (`spike`)            | 50, 2000                 | Tasa normal antes y después del pico, y la del pico                                                           |
| `ATTACK_RATE`, `VICTIM_RATE`, `SELFTEST`                    | isolation                        | 20, 8, —                 | Iteraciones por segundo de atacantes y víctimas; `SELFTEST=1` para el autotest                                |
| `LT_SOAK_TENANTS`, `LT_SOAK_MONITORS`, `LT_SOAK_INTERVAL_S` | checker-soak y su semilla        | 200, 10, 30              | Tamaño del set del soak (tenants × monitores, cada cuántos segundos)                                          |
| `WARMUP`                                                    | checker-soak                     | `3m`                     | Espera antes de medir: un intervalo para repartir los monitores y una ventana completa de `rate()`            |
| `DURATION`                                                  | api-crud, status-page, isolation | `5m` (`3m` en isolation) | Duración de la fase estable                                                                                   |
| `BASE_URL` o `API_URL`, `AUTH_URL`, `STATUS_URL`            | todos                            | los de compose           | Contra qué entorno corre                                                                                      |
| `ECHO_URL`                                                  | api-crud, smoke, semilla         | `http://echo/`           | Destino de los monitores                                                                                      |

## Datos de prueba

`just loadtest-seed` inserta con psql, como el rol dueño (que no pasa por RLS), lo que el
registro no permite crear rápido: registrarse exige verificar el correo y auth acepta 10
registros por IP y hora. Todo lleva el prefijo `lt-` en el slug y el dominio
`@loadtest.taut.test` en el correo, y `just loadtest-clean` lo borra.

| Grupo                                     | Tenants | Monitores                                        | Historia        | Para                      |
| ----------------------------------------- | ------- | ------------------------------------------------ | --------------- | ------------------------- |
| `lt-s-01`…`lt-s-20`                       | 20      | 10 cada uno, cada 5 min                          | 89 días a 5 min | `status-page`             |
| `lt-p-01`…`lt-p-50`                       | 50      | 5 cada uno, cada 1 min                           | 30 días a 1 min | `api-crud`, `smoke`       |
| `lt-rl-a`, `lt-rl-b`, `lt-i-a1`…`lt-i-b4` | 10      | 3 cada uno, cada 5 min, con el slug en el nombre | 1 día a 5 min   | `rate-limit`, `isolation` |

`seed/mixed.sql` (lo corre `vps-demo.sh mixta-sembrar`) solo agrega o reemplaza sus 20 tenants
`lt-m-01`… con dueño, 10 monitores HTTP cada 60 s y 7 días de historia a 5 min; no toca el resto
de los datos. `seed/mixed-clean.sql` los borra.

`just loadtest-seed soak` reemplaza todo lo anterior por 200 tenants `lt-k-0001`… con 10 monitores cada uno, cada 30 s y sin historia ni usuarios: 2 000 monitores y 66,7 chequeos por segundo. Respeta la cuota de 10 monitores por tenant del plan free.

Son unos 16 millones de filas en `check_results` (~2,5 GB). Cada monitor tiene además tres
incidentes resueltos, uno en la última semana. Los monitores apuntan al servidor eco y siguen
activos, así que el checker los vigila de verdad durante las pruebas (unos 5 chequeos por
segundo de carga de fondo, como en producción). Los tamaños se cambian con `LT_STATUS_TENANTS`,
`LT_STATUS_MONITORS`, `LT_STATUS_DAYS`, `LT_STATUS_STEP_MIN`, `LT_PANEL_TENANTS`,
`LT_PANEL_MONITORS`, `LT_PANEL_DAYS` y `LT_PANEL_STEP_MIN`; `just loadtest` le pasa los mismos
valores a k6.

Todos los usuarios tienen la contraseña de desarrollo `loadtest-only-password` (`LT_PASSWORD`).
Los escenarios hacen login una vez en `setup()`: el hash Argon2id es lento a propósito y medirlo
en cada iteración sería medir auth. Nunca mandes una contraseña errónea: auth bloquea una IP tras
20 logins fallidos en 15 minutos, y todas las peticiones de k6 salen de la misma IP.

## Medir bien

- Imágenes release (`just loadtest-up`), nunca `just dev` ni `just workers`, que compilan en debug.
- Configuración por defecto (pool de 10 conexiones, logs `info`). La caché de la página de
  estado viene activa (30 s); para la línea base sin ella, `TAUT_STATUS__CACHE_TTL_SECS=0 just
  loadtest-up` (vuelve a activarla un `just loadtest-up` sin la variable). Si una prueba
  encuentra un cuello de botella, se reporta con su evidencia y se arregla en otro PR: la línea
  base tiene que poder compararse con la de la semana 13.
- Anota la máquina y los recursos de la VM de OrbStack en el reporte: k6 y Taut comparten CPU.
- Corre `smoke` antes de cada prueba pesada y no uses el panel mientras tanto.
- Anota la hora UTC de las corridas de `status-page`: el día en curso de `uptime_daily` se
  calcula en cada lectura desde `check_results` y crece hasta la medianoche UTC
  ([perfilado](../docs/loadtest/2026-10-20-status-bff-perfil.md)). `just loadtest-seed` lo deja
  sin materializar, como la política en producción.
- En macOS, corre las pruebas largas con `caffeinate -i just loadtest …`. Si el Mac entra en
  reposo a mitad de la corrida, los contenedores se congelan y esa corrida no sirve (pasó con el
  primer `stress` del 19-10: hubo que repetirlo).

## Estructura

```
lib/config.js       variables de entorno, tamaños de la semilla y nombres (lt-s-NN, lt-p-NN)
lib/auth.js         login de los usuarios sembrados, renovación del token y API keys
lib/http.js         peticiones a la api con Bearer y X-Tenant-Id; contador rate_limited
lib/checks.js       umbrales de la fase estable
lib/prom.js         consultas a Prometheus (contadores del servidor que deciden un umbral)
scenarios/*.js      un archivo por escenario
seed/base.sql       semilla; soak.sql la del checker-soak; mixed.sql la de mixed (aditiva, la borra mixed-clean.sql); clean.sql las borra; refresh.sql actualiza agregados y estadísticas
vps-demo.sh         las pruebas contra la VPS de la demo (página de estado, caché y mixta), con Grafana local
```

Las rutas se etiquetan con su patrón (`GET /v1/monitors/:id`), nunca con la URL real, para no
crear una serie de Prometheus por monitor. `just loadtest-lint` revisa los scripts con
`k6 inspect` sin correrlos.

## Pendiente

- El sondeo de 30 000 monitores agota los puertos efímeros del checker (todos apuntan al mismo
  eco): repartir los monitores del soak entre varios destinos, o darle al checker
  `tcp_tw_reuse` y el rango de puertos ampliado, y agregar un umbral sobre los fallos de los
  monitores `lt-k-*` ([reporte](../docs/loadtest/2026-10-20-checker-puertos.md)).
- La parte del HPA del soak (que el checker escale) espera a Kubernetes.
- Semana 13: repetir `status-page` en el Mac con la caché y sin ella (`TAUT_STATUS__CACHE_TTL_SECS=0`),
  a 500 y a 900 req/s. En la VPS ya se comparó ([reporte](../docs/loadtest/2026-10-21-vps-cache.md)).
- Job nightly contra staging (falla si p95 > 300 ms) cuando exista staging.
