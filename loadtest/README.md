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

| Escenario     | Qué mide                                                                                    | Carga por defecto                                                                 | Umbrales                                                             |
| ------------- | ------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `smoke`       | Que el stack, la semilla y los scripts funcionan                                            | 1 VU, 3 iteraciones                                                               | todos los checks y ninguna petición fallida                          |
| `api-crud`    | El panel: lista, detalle, checks, métricas e incidentes, más crear, editar, pausar y borrar | 1 min de calentamiento a 5 it/s; 5 min a 25 it/s de lectura y 1 it/s de escritura | p95 < 300 ms (global y por ruta), p99 < 1 s, errores < 1 %, cero 429 |
| `status-page` | Páginas de estado públicas, sin caché                                                       | 30 s de rampa y 5 min a 500 req/s                                                 | p95 < 300 ms, p99 < 1 s, errores < 1 %                               |

Los escenarios usan un **modelo abierto** (`constant-arrival-rate`): k6 lanza un número fijo de
iteraciones por segundo, haya respondido o no el sistema. Si Taut se pone lento la carga no baja:
sube la latencia y aparecen `dropped_iterations`, que también es un umbral. Los umbrales solo miran
la fase `steady`, no el calentamiento.

Variables de los escenarios:

| Variable                                         | Escenario                | Por defecto    | Para qué                                                                                                      |
| ------------------------------------------------ | ------------------------ | -------------- | ------------------------------------------------------------------------------------------------------------- |
| `READ_RATE`, `WRITE_RATE`                        | api-crud                 | 25, 1          | Iteraciones por segundo; el script se niega a pasar del 80 % del presupuesto de 600 peticiones/min por tenant |
| `RATE`                                           | status-page              | 500            | Peticiones por segundo en la fase estable                                                                     |
| `SLUGS`                                          | status-page              | `spread`       | `spread` reparte entre las 20 páginas; `hot` las manda todas a una                                            |
| `DURATION`                                       | api-crud, status-page    | `5m`           | Duración de la fase estable                                                                                   |
| `BASE_URL` o `API_URL`, `AUTH_URL`, `STATUS_URL` | todos                    | los de compose | Contra qué entorno corre                                                                                      |
| `ECHO_URL`                                       | api-crud, smoke, semilla | `http://echo/` | Destino de los monitores                                                                                      |

## Datos de prueba

`just loadtest-seed` inserta con psql, como el rol dueño (que no pasa por RLS), lo que el
registro no permite crear rápido: registrarse exige verificar el correo y auth acepta 10
registros por IP y hora. Todo lleva el prefijo `lt-` en el slug y el dominio
`@loadtest.taut.test` en el correo, y `just loadtest-clean` lo borra.

| Grupo               | Tenants | Monitores               | Historia        | Para                |
| ------------------- | ------- | ----------------------- | --------------- | ------------------- |
| `lt-s-01`…`lt-s-20` | 20      | 10 cada uno, cada 5 min | 89 días a 5 min | `status-page`       |
| `lt-p-01`…`lt-p-50` | 50      | 5 cada uno, cada 1 min  | 30 días a 1 min | `api-crud`, `smoke` |

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
- Configuración por defecto (pool de 10 conexiones, logs `info`, sin caché). Si una prueba
  encuentra un cuello de botella, se reporta con su evidencia y se arregla en otro PR: la línea
  base tiene que poder compararse con la de la semana 13.
- Anota la máquina y los recursos de la VM de OrbStack en el reporte: k6 y Taut comparten CPU.
- Corre `smoke` antes de cada prueba pesada y no uses el panel mientras tanto.

## Estructura

```
lib/config.js       variables de entorno, tamaños de la semilla y nombres (lt-s-NN, lt-p-NN)
lib/auth.js         login de los usuarios sembrados y renovación del token
lib/http.js         peticiones a la api con Bearer y X-Tenant-Id; contador rate_limited
lib/checks.js       umbrales de la fase estable
scenarios/*.js      un archivo por escenario
seed/base.sql       semilla; clean.sql la borra; refresh.sql actualiza agregados y estadísticas
```

Las rutas se etiquetan con su patrón (`GET /v1/monitors/:id`), nunca con la URL real, para no
crear una serie de Prometheus por monitor. `just loadtest-lint` revisa los scripts con
`k6 inspect` sin correrlos.

## Pendiente

- Fase 2: `rate-limit` (429 y `Retry-After` al pasar el presupuesto), `isolation` (ningún tenant
  ve datos de otro bajo carga) y los perfiles `stress`, `spike` y `breakpoint` de `status-page`.
- Fase 3: `checker-soak`, 2 000 monitores a 30 s durante 30 min, con `uptime_check_lag_seconds`
  bajo 10 s; la parte del HPA espera a Kubernetes.
- Semana 13: repetir `status-page` con la caché de Redis y comparar con la línea base.
- Job nightly contra staging (falla si p95 > 300 ms) cuando exista staging.
