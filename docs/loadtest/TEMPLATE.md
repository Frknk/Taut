# Reporte de carga: <escenario> (<fecha>)

<!-- Copia este archivo a docs/loadtest/<AAAA-MM-DD>-<escenario>.md. Las capturas van en
     docs/loadtest/img/<AAAA-MM-DD>-<escenario>/ y el summary.json de la corrida en
     docs/loadtest/data/<testid>.json (just loadtest-keep <testid>, que quita los tokens).
     Borra estos comentarios al terminar. -->

| Campo     | Valor                               |
| --------- | ----------------------------------- |
| Fecha     | AAAA-MM-DD HH:MM UTC                |
| Commit    | `abc1234` (rama)                    |
| testid    | `AAAAMMDDTHHMMSSZ-<escenario>`      |
| k6        | 2.3.0 en contenedor, red de compose |
| Veredicto | Pasa / No pasa (qué umbral)         |

## Resumen

<!-- Tres o cuatro frases: qué se midió, si cumple el SLO y el hallazgo principal. -->

## Máquina

| Campo          | Valor                                           |
| -------------- | ----------------------------------------------- |
| Equipo         | p. ej. MacBook Pro M4, 10 núcleos, 16 GB        |
| SO             | macOS …                                         |
| Docker         | OrbStack … (CPU y memoria de la VM)             |
| Dónde corre k6 | contenedor `grafana/k6` en la misma VM que Taut |

## Configuración

- Imágenes release del commit indicado (`just loadtest-up`).
- `TAUT_DATABASE__MAX_CONNECTIONS` = 10, `TAUT_CHECKS__CONCURRENCY` = 100, logs `info`, sin caché.
- Semilla: <tamaños> → <filas reales de check_results> (`just loadtest-seed`).
- Carga de fondo: <monitores activos> monitores sembrados chequeándose.

## Escenario

<!-- Executor, fases, tasas, mezcla de peticiones, variables (-e …) y umbrales. -->

## Resultados (fase estable)

| Ruta     | req/s | p50 | p90 | p95 | p99 | max | errores | 429 |
| -------- | ----: | --: | --: | --: | --: | --: | ------: | --: |
| `GET /…` |       |     |     |     |     |     |         |     |

- Iteraciones descartadas (`dropped_iterations`): …
- VUs máximos usados: …
- Umbrales: …

## Lado servidor

| Medida                              | Valor | Fuente                                                    |
| ----------------------------------- | ----- | --------------------------------------------------------- |
| p95 por ruta en el servidor         |       | `http_request_duration_seconds` (tablero Taut: servicios) |
| Conexiones de PostgreSQL (pico)     |       | `pg_stat_activity_count`                                  |
| CPU y memoria por contenedor (pico) |       | `docker stats`                                            |
| Lag de chequeos de fondo (p95)      |       | `uptime_check_lag_seconds`                                |

<!-- La diferencia entre el p95 de k6 (cliente) y el del servidor es tiempo en cola fuera del
     handler: conexiones, red de la VM o k6 saturado. -->

## Capturas

<!-- ![Latencia](img/<fecha>-<escenario>/latencia.png) -->

## Cuellos de botella

| Síntoma | Evidencia | Hipótesis | Arreglo propuesto |
| ------- | --------- | --------- | ----------------- |
|         |           |           | issue #…          |

## Comparación

<!-- Con la corrida anterior del mismo escenario (o la línea base de la semana 6). -->

## Cómo reproducir

```sh
git checkout <commit>
just loadtest-up
just loadtest-seed
just loadtest smoke
just loadtest <escenario> <variables>
```
