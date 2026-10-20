# Reporte de carga: api-crud con el uptime de `uptime_daily` (2026-10-20)

| Campo     | Valor                                                                                 |
| --------- | ------------------------------------------------------------------------------------- |
| Fecha     | 2026-10-20, 05:01–05:16 UTC                                                           |
| Commit    | antes: `main` en `5f5699d`; después: rama `perf/api-metrics-uptime-daily` (`acd1902`) |
| testid    | antes: `20261020T050117Z-api-crud`; después: `20261020T050946Z-api-crud`              |
| k6        | 2.3.0 en contenedor, red de compose                                                   |
| Veredicto | **Pasa** los dos; `/metrics` baja de 41 a 21 ms de p50 y el p95 global de 47 a 26 ms  |

## Resumen

La [línea base del 19-10](2026-10-19-api-crud.md) encontró que `GET /v1/monitors/:id/metrics`
costaba diez veces más que el resto de las lecturas: cada respuesta trae el uptime de 24 h, 7 d
y 30 d, así que el conteo recorría 30 días de `check_results` (43 200 filas por monitor a 1 min)
aunque se pidiera `range=24h`. Ahora las ventanas de 7 y 30 días suman los días completos de
`uptime_daily` (función `uptime_daily_of_monitors`) a los resultados crudos de su primer día
parcial, y el conteo sigue siendo exacto. Con la misma carga, el p50 de `/metrics` se reduce a la
mitad y PostgreSQL usa un 35 % menos de CPU.

## Máquina y condiciones

- Apple M4, 16 GB, OrbStack 2.2.4 con 10 CPU y 7,8 GiB; imágenes release y configuración por
  defecto.
- **Conectado a la corriente, con el modo de bajo consumo activo** (`pmset` lo tiene en 1 también
  para AC). Las dos corridas se hicieron seguidas, con el Mac sin usar (más del 83 % de CPU libre
  en el host antes de cada una). No se comparan con las del 19-10, que fueron a batería.
- Semilla base (`just loadtest-seed`): 50 tenants `lt-p-*` con 5 monitores cada uno y 30 días de
  historia a 1 min.
- Lo único que cambia entre las dos corridas es la imagen de la api.

## Escenario

`just loadtest api-crud` con los valores por defecto, igual que el 19-10: 1 min de calentamiento
y 5 min a 25 it/s de lectura (lista, detalle, checks, métricas con `24h` 70 %, `7d` 20 % y `30d`
10 %, e incidentes) y 1 it/s de escritura. 135 req/s en la fase estable.

## Resultados (fase estable, 300 s)

| Ruta                           | p50 antes | p50 después | p95 antes | p95 después | p99 antes | p99 después |
| ------------------------------ | --------: | ----------: | --------: | ----------: | --------: | ----------: |
| `GET /v1/monitors/:id/metrics` |   41,2 ms |     20,8 ms |     76 ms |     56,9 ms |    146 ms |       66 ms |
| `GET /v1/monitors`             |    2,5 ms |      2,4 ms |    5,7 ms |      5,3 ms |     30 ms |      8,0 ms |
| `GET /v1/monitors/:id`         |    2,2 ms |      2,1 ms |    4,7 ms |      3,7 ms |     18 ms |      6,4 ms |
| `GET /v1/monitors/:id/checks`  |    6,7 ms |      6,7 ms |   10,8 ms |      9,8 ms |     44 ms |       14 ms |
| `GET /v1/incidents`            |    1,0 ms |      0,9 ms |    3,2 ms |      2,4 ms |     10 ms |      4,0 ms |
| Escrituras (4 rutas)           |    5–9 ms |      5–9 ms |  11–14 ms |    10–15 ms |  40–75 ms |    13–21 ms |
| **Total**                      |    2,9 ms |      2,7 ms |   47,2 ms |     25,9 ms |   77,9 ms |     56,5 ms |

- Las dos corridas: 0 errores, 0 respuestas 429, 0 iteraciones descartadas, todos los checks
  correctos y la misma cantidad de peticiones por ruta (7 500 de cada lectura).
- CPU media en la fase estable: PostgreSQL **155 % → 101 %**; la api 8 % en las dos.
- Los p99 del resto de las rutas también bajan: con `/metrics` más corto, las demás consultas
  esperan menos a PostgreSQL.

## Qué cuesta cada consulta

`EXPLAIN ANALYZE` del conteo para un monitor de `lt-p-07`, con la sesión caliente:

| Consulta                                                        | Antes                | Después |
| --------------------------------------------------------------- | -------------------- | ------- |
| Conteo de 24 h, 7 d y 30 d sobre `check_results` (43 200 filas) | 9,7 ms (110 en frío) | —       |
| Crudos: 24 h más el primer día parcial de cada ventana          | —                    | 0,8 ms  |
| Días completos desde `uptime_daily_of_monitors`                 | —                    | 0,6 ms  |

## Cuellos de botella

| Síntoma                                          | Evidencia                               | Hipótesis                                                                                                | Arreglo propuesto                                                                                |
| ------------------------------------------------ | --------------------------------------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `/metrics` sigue siendo la ruta más cara (21 ms) | p50 diez veces el de las demás lecturas | La serie y los percentiles de `7d` y `30d` leen 7 y 30 días crudos; `percentile_cont` necesita las filas | Semana 10 (CQRS): una proyección horaria para las series largas, medida con este mismo escenario |

## Comparación

Con la línea base del 19-10 (p50 40 ms y p95 71 ms en `/metrics`, a batería) la corrida «antes»
de hoy coincide: 41 y 76 ms. La mejora se mide entre las dos corridas de hoy.

## Cómo reproducir

```sh
just loadtest-up
just loadtest-seed
just loadtest smoke
just loadtest api-crud
```
