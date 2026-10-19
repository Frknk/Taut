# Reporte de carga: isolation (2026-10-19)

| Campo     | Valor                                                                                                     |
| --------- | --------------------------------------------------------------------------------------------------------- |
| Fecha     | 2026-10-19 19:21 UTC                                                                                      |
| Commit    | imágenes de `92928be` (sin cambios de código desde entonces); scripts de la rama `test/loadtest-security` |
| testid    | `20261019T192121Z-isolation` (autotest: `20261019T192428Z-isolation`)                                     |
| k6        | 2.3.0 en contenedor, red de compose                                                                       |
| Veredicto | **Pasa**: ninguna fuga en 9 365 comprobaciones                                                            |

## Resumen

Durante 3 minutos, cuatro tenants atacantes intentaron llegar a los datos de su víctima por 14
caminos distintos mientras las víctimas usaban el panel. Ninguna respuesta le dio al atacante
algo ajeno: todas fueron la respuesta correcta (403, 404 o una lista sin datos ajenos), y la api
responde igual ante un tenant existente que ante uno inventado. El autotest, que apunta el detector contra el propio tenant,
falló como debía: 45 fugas detectadas y la corrida se abortó sola. Así que el resultado limpio
no se debe a un detector ciego.

## Máquina y configuración

Igual que la [línea base](2026-10-19-api-crud.md). La api usa el rol `taut_app` con Row-Level
Security por tenant ([ADR 0010](../adr/0010-row-level-security-por-tenant.md)).

## Escenario

`just loadtest isolation`. La semilla crea cuatro pares atacante/víctima (`lt-i-a1`…`lt-i-a4`
contra `lt-i-b1`…`lt-i-b4`), con 3 monitores cada uno cuyo nombre lleva el slug del tenant: un
nombre ajeno en una respuesta delata la fuga. En `setup()` cada atacante obtiene una API key
propia y anota los ids de los monitores e incidentes de su víctima.

- `attack`: 20 it/s durante 3 min. Cada iteración prueba un camino distinto, en rueda.
- `victims`: 8 it/s. Cada víctima lista sus monitores, abre uno y pide sus métricas. Así las
  conexiones del pool cambian de tenant entre una petición y la siguiente: si el contexto de RLS
  se filtrara de una conexión a otra, aparecería aquí.
- Una fuga (`isolation_violations`) aborta la corrida al primer caso. Cualquier otra respuesta
  inesperada, como un 5xx o un 429, cuenta aparte (`unexpected_responses`).

## Resultados

| Camino del atacante                         | Respuesta esperada     | Comprobaciones | Fugas |
| ------------------------------------------- | ---------------------- | -------------: | ----: |
| `X-Tenant-Id` de la víctima                 | 403 `not_a_member`     |            361 |     0 |
| Misma respuesta que con un tenant inventado | mismo código y mensaje |            361 |     0 |
| Detalle de un monitor ajeno                 | 404                    |            360 |     0 |
| Chequeos de un monitor ajeno                | 404                    |            360 |     0 |
| Métricas de un monitor ajeno                | 404                    |            360 |     0 |
| Editar un monitor ajeno (PUT)               | 404                    |            360 |     0 |
| Pausar un monitor ajeno                     | 404                    |            360 |     0 |
| Borrar un monitor ajeno                     | 404                    |            360 |     0 |
| Incidente ajeno                             | 404                    |            360 |     0 |
| Incidentes filtrados por un monitor ajeno   | 200, lista vacía       |            360 |     0 |
| API key propia con `X-Tenant-Id` ajeno      | 403 `forbidden`        |            360 |     0 |
| API key propia sobre un monitor ajeno       | 404                    |            360 |     0 |
| Lista de monitores propia                   | sin ids ajenos         |            360 |     0 |
| Página de estado propia                     | sin monitores ajenos   |            360 |     0 |

Las víctimas hicieron 4 323 peticiones (1 441 por cada tipo de lectura), todas con 200. En total
hubo 9 397 peticiones a 52 req/s, con un p95 de 19 ms y ninguna respuesta inesperada.

## Autotest

`just loadtest isolation -e SELFTEST=1` hace que cada atacante ataque su propio tenant. El
detector tiene que ver «fugas» por todas partes, y las vio: 45 en las primeras 52 iteraciones,
y el umbral con `abortOnFail` cortó la corrida en un segundo. El autotest omite los caminos que
modifican datos (editar, pausar, borrar), para no tocar los datos sembrados.

## Cómo reproducir

```sh
just loadtest-up
just loadtest-seed
just loadtest smoke
just loadtest isolation
just loadtest isolation -e SELFTEST=1   # tiene que fallar
```
