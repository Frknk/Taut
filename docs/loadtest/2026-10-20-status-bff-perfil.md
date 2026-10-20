# Perfilado de status-bff (2026-10-20)

| Campo     | Valor                                                                                                                                           |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Fecha     | 2026-10-20, 03:48–04:20 UTC                                                                                                                     |
| Commit    | `main` en `5f5699d` (antes); ramas `perf/status-page-uptime-by-monitor` («por monitor») y `perf/telemetry-static-log-filter` («filtro de logs») |
| testid    | antes: `20261020T034807Z-status-page`; por monitor: `20261020T040545Z-status-page`; con los dos: `20261020T041834Z-status-page`                 |
| k6        | 2.3.0 en contenedor, red de compose                                                                                                             |
| Veredicto | Dos causas, las dos arregladas: el agregado en tiempo real sin filtro por monitor (PostgreSQL) y el `RwLock` de `EnvFilter` (status-bff)        |

## Resumen

El 19-10 la página de estado se estancaba cerca de 920 req/s sin agotar la CPU ni el pool, y
quedó pendiente perfilar status-bff. Al repetir el diagnóstico el 20-10 la página solo atendía
**325 req/s**, con PostgreSQL al 645 % de CPU. Había dos problemas:

1. **PostgreSQL.** La parte del día en curso de `uptime_daily` (en tiempo real, sin materializar)
   leía todas las filas del día de todos los tenants para quedarse con las de uno: no hay índice
   que empiece por `tenant_id`. Su costo crece a lo largo del día UTC y con el volumen de toda la
   plataforma. El 19-10 no se vio porque la semilla materializaba también el día en curso.
2. **status-bff.** El 65 % de su CPU se iba en `tracing_subscriber::EnvFilter`, que toma un
   `RwLock` global cada vez que un span se entra o se sale. sqlx entra al span de la petición una
   vez por fila, y la página trae 900. Eso explica que no pasara de 3–4,5 núcleos con CPU libre.

Con los dos arreglos, la misma carga (1 200 req/s ofrecidas) pasa de 325 a **1 154 req/s**, con
p50 de 3,8 ms.

| Corrida (1 200 req/s ofrecidas, 90 s) |   Atendidas |      p50 |      p95 | CPU status-bff | CPU PostgreSQL |
| ------------------------------------- | ----------: | -------: | -------: | -------------: | -------------: |
| `main`                                |   325 req/s | 3 040 ms | 4 011 ms |           67 % |          645 % |
| Uptime por monitor                    |   778 req/s | 1 271 ms | 1 449 ms |          185 % |          349 % |
| Además, filtro de logs estático       | 1 154 req/s |   3,8 ms |   937 ms |           95 % |          274 % |

CPU media de `docker stats` en la fase estable; 100 % es un núcleo. Por página, status-bff pasó de
~2,4 ms de CPU (por monitor) a ~0,8 ms (con los dos), y PostgreSQL de ~20 ms (`main`) a ~2,4 ms.

## Máquina y condiciones

- Apple M4 (4 núcleos de rendimiento y 6 de eficiencia), 16 GB, OrbStack 2.2.4 con 10 CPU y
  7,8 GiB, imágenes release, configuración por defecto (pool de 10, logs `info`, sin caché).
- **A batería con el modo de bajo consumo** (25 % → 20 %), igual que el 19-10. Las tres corridas
  de la tabla se hicieron seguidas y en las mismas condiciones.
- Semilla base del 19-10 (`just loadtest-seed`), unos 16 millones de filas en `check_results`.
  Las corridas fueron entre las 03:48 y las 04:20 UTC: el chunk del día tenía ~19 000 filas.

## Hallazgo 1: el día en curso de `uptime_daily`

`uptime_daily` es un agregado continuo con `materialized_only = false`: la política materializa
los días completos y la parte del día en curso se calcula en cada lectura desde `check_results`.
`EXPLAIN ANALYZE` de `uptime_daily_of_current_tenant` para `lt-s-01`:

- La parte materializada (89 días, 890 filas) usa el índice `(tenant_id, day)` y tarda 0,6 ms.
- La parte en tiempo real hace un **seq scan del chunk de hoy**: 18 952 filas descartadas para
  quedarse con 129, y evalúa `current_setting('app.tenant_id')` y el cast a `uuid` en cada fila.
  Tarda 13 ms.
- La función entera, ~10,5 ms por llamada, con la sesión caliente.

`check_results` solo tiene la clave primaria `(monitor_id, at, region)` y `(at)`. Filtrando por
tenant, el costo de cada página crece con el volumen de toda la plataforma y a lo largo del día
UTC. Con 2 000 monitores a 30 s, el chunk de hoy llega a 5,7 millones de filas a medianoche.

**Por qué no se vio el 19-10.** `loadtest/seed/refresh.sql` llamaba a
`refresh_continuous_aggregate('uptime_daily', NULL, NULL)`. Con el final en `NULL`, TimescaleDB
materializa también el día en curso y deja la marca de agua en la medianoche siguiente, así que
la parte en tiempo real quedó vacía todo el 19-10. Se comprobó en una base aparte: con el final en
`now() - 1 hour` (el de la política) la marca de agua queda en la medianoche de hoy, y con `NULL`,
en la de mañana. En producción la marca de agua siempre está en la medianoche de hoy: los ~920
req/s del 19-10 se midieron en un estado que producción nunca tiene.

**Arreglo: uptime por monitor.** Se compararon dos opciones:

| Opción                                                | Llamada | Costo                                                                     |
| ----------------------------------------------------- | ------: | ------------------------------------------------------------------------- |
| Índice `check_results (tenant_id, at)`                |  1,5 ms | +622 MB en dev (+20 % del hypertable) y un índice más en cada insert      |
| Función que recibe los ids de los monitores (elegida) |  2,0 ms | Ninguno: la parte en tiempo real usa la clave primaria `(monitor_id, at)` |

La migración `uptime_daily_of_monitors(since, monitors[])` filtra además por monitor, y
status-bff le pasa los ids que ya leyó. La garantía de RLS es la misma (`current_tenant_id()`), y
el test de RLS comprueba que ids de otro tenant no abren sus filas. `uptime_daily_of_current_tenant`
se queda para los binarios que todavía la llaman. `refresh.sql` ahora termina la ventana donde la
termina la política, para que el día en curso siga en tiempo real como en producción.

## Hallazgo 2: `EnvFilter` en status-bff

Con el uptime por monitor, status-bff gastaba ~2,4 ms de CPU por página. Se perfiló un binario release con
símbolos, nativo en macOS, contra el PostgreSQL de compose, con `sample` durante 30 s a ~380
req/s:

| Dónde                                                          | Muestras ocupadas |
| -------------------------------------------------------------- | ----------------: |
| `tracing` y `tracing_subscriber` en total                      |              72 % |
| …de eso, `EnvFilter::on_enter` y `EnvFilter::cares_about_span` |              65 % |
| sqlx (lectura y decodificación de filas)                       |              14 % |
| serde_json (los ~66 KB de la respuesta)                        |               1 % |

- sqlx 0.9 envuelve el stream de filas en `in_current_span()`, así que cada fila entra y sale del
  span de la petición.
- `EnvFilter` toma `by_id.read()`, un `RwLock` que comparten todos los hilos, en cada entrada y en
  cada salida, aunque `RUST_LOG` no tenga directivas por span.
- Con 10 hilos, la línea de caché del lock rebota entre núcleos. Por eso status-bff no usaba más
  de 3–4,5 núcleos aunque hubiera CPU libre.

Los candidatos del 19-10 quedan descartados: escribir el log por petición y serializar los 66 KB
no llegan al 2 % entre los dos.

**Arreglo: filtro de logs estático.** `taut_telemetry` usa un filtro estático (`Targets`) cuando `RUST_LOG` solo
trae niveles por target (`info,sqlx=warn`), y `EnvFilter` solo cuando una directiva filtra por
span o campo (lleva corchetes). Afecta a todos los servicios.

## Cuellos de botella

| Síntoma                                        | Evidencia                                                | Causa                                                                  | Arreglo                                                   |
| ---------------------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------- | --------------------------------------------------------- |
| La página cae a 325 req/s; PostgreSQL al 645 % | Seq scan de 19 000 filas por página en el chunk de hoy   | El día en curso de `uptime_daily` se filtraba solo por tenant          | `uptime_daily_of_monitors` con los ids                    |
| La línea base del 19-10 era optimista          | Marca de agua en la medianoche siguiente tras la semilla | `refresh_continuous_aggregate(…, NULL, NULL)` en `refresh.sql`         | La ventana termina como la de la política                 |
| status-bff no pasa de 3–4,5 núcleos            | 65 % de la CPU en `EnvFilter`                            | `RwLock` global en cada entrada a un span; sqlx entra una vez por fila | Filtro estático salvo directivas por span                 |
| PostgreSQL sigue siendo el mayor consumidor    | ~2,4 ms de CPU por página con los dos                    | 4 consultas y una transacción por página                               | Semana 13: la caché de Redis evita el armado de la página |

## Comparación

Contra el 19-10 los números no son comparables directamente: aquel día la parte en tiempo real
estaba vacía. Comparado con la misma semilla, la misma hora y las mismas condiciones, con los dos arreglos atiende
3,5 veces más que `main`, y más de lo que atendía `main` el 19-10 con la parte en tiempo real
vacía (728–795 req/s en el diagnóstico equivalente). Para la semana 13 conviene repetir la línea
base con los dos arreglos fusionados, a una hora del día UTC anotada en el reporte.

## Cómo reproducir

```sh
just loadtest-up
just loadtest-seed
just loadtest smoke
just loadtest status-page -e RATE=1200 -e DURATION=90s -e ACCEPT_ENCODING=
```

La marca de agua no retrocede: si la base se sembró alguna vez con el `refresh.sql` anterior, la
parte en tiempo real queda vacía hasta la medianoche UTC siguiente. Se comprueba con
`SELECT _timescaledb_functions.to_timestamp(_timescaledb_functions.cagg_watermark(mat_hypertable_id)) FROM _timescaledb_catalog.continuous_agg WHERE user_view_name = 'uptime_daily'`,
que tiene que dar la medianoche de hoy.

El perfil: compilar status-bff con
`CARGO_PROFILE_RELEASE_STRIP=false CARGO_PROFILE_RELEASE_DEBUG=line-tables-only cargo build --release -p taut-status-bff`,
arrancarlo con `TAUT_HTTP__ADDR=127.0.0.1:18085` y el `DATABASE_URL` de `localhost`, cargarlo con
`-e STATUS_URL=http://host.docker.internal:18085` y muestrearlo con `sample <pid> 30`.
