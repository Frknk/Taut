# Reporte de carga: status-page con compresión (2026-10-20)

| Campo     | Valor                                                                                                                                                 |
| --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fecha     | 2026-10-20, 06:29–06:41 UTC                                                                                                                           |
| Commit    | status-bff con la compresión de `perf/http-compression` más los dos arreglos del perfilado                                                            |
| testid    | sin comprimir: `20261020T062948Z-status-page`, `20261020T063453Z-status-page`; brotli: `20261020T063220Z-status-page`, `20261020T063855Z-status-page` |
| k6        | 2.3.0 en contenedor, red de compose                                                                                                                   |
| Veredicto | **Pasa**: 19,5 veces menos bytes por ~0,3 ms de CPU por página en status-bff                                                                          |

## Resumen

Las páginas de estado salían sin comprimir: ~64 KB de JSON por respuesta. Ahora la capa HTTP
compartida comprime con brotli (calidad 4) o gzip según `Accept-Encoding`, y el escenario
`status-page` manda ese encabezado como un navegador por HTTPS (`gzip, deflate, br`); k6 no lo
manda si no se le pide. A 500 req/s las dos variantes cumplen el SLO con holgura. Con brotli
cada respuesta pasa de 64 546 a 3 311 bytes (19,5 veces menos), a cambio de ~0,3 ms de CPU por
página en status-bff y medio milisegundo más de p50.

## Máquina y condiciones

- Apple M4, 16 GB, OrbStack 2.2.4 con 10 CPU y 7,8 GiB; imágenes release y configuración por
  defecto.
- Conectado a la corriente con el modo de bajo consumo activo y el Mac sin usar.
- **Una sola imagen de status-bff para todas las corridas.** Solo cambia el encabezado:
  `-e ACCEPT_ENCODING=` pide la página sin comprimir y el valor por defecto la pide con brotli.
  Las corridas se alternan (sin, con, sin, con) para que la deriva de la máquina afecte a las dos.
- La imagen lleva también los dos arreglos del [perfilado](2026-10-20-status-bff-perfil.md). Para
  esta comparación da igual, porque los dos lados los tienen.
- Semilla base. Esta vez la parte en tiempo real de `uptime_daily` estaba vacía (la base se había
  sembrado con el `refresh.sql` anterior), así que PostgreSQL trabajó menos que en producción.

## Tamaño y costo de comprimir

| Codificación               | Bytes de la página `lt-s-01` | Tiempo por página (en el host) |
| -------------------------- | ---------------------------: | -----------------------------: |
| Sin comprimir              |                       64 392 |                              — |
| brotli calidad 4 (elegida) |                        3 008 |                         156 µs |
| gzip nivel 6               |                        2 740 |                         371 µs |
| gzip nivel 1               |                        4 743 |                          79 µs |

Tiempos medidos con las bibliotecas de C desde Python sobre la misma página, para elegir el nivel.
brotli calidad 4 es el valor por defecto de tower-http para respuestas dinámicas (el de la
biblioteca, 11, es demasiado lento).

## Resultados (fase estable de 2 min a 500 req/s)

| Corrida (2 min)  | Atendidas | Bytes por respuesta |    p50 |    p95 |    p99 | CPU status-bff | CPU PostgreSQL |
| ---------------- | --------: | ------------------: | -----: | -----: | -----: | -------------: | -------------: |
| 1, sin comprimir | 500 req/s |              64 546 | 3,5 ms | 5,5 ms | 7,0 ms |           56 % |          123 % |
| 1, brotli        | 500 req/s |               3 311 | 4,0 ms | 6,3 ms | 8,8 ms |           71 % |          126 % |
| 2, sin comprimir | 500 req/s |              64 546 | 3,6 ms | 5,8 ms | 7,8 ms |           54 % |          118 % |
| 2, brotli        | 500 req/s |               3 311 | 4,0 ms | 6,2 ms | 8,1 ms |           71 % |          127 % |

- 0 errores y 0 iteraciones descartadas en las cuatro.
- Las dos parejas coinciden: brotli suma ~15 puntos de CPU a status-bff a 500 req/s, unos 0,3 ms
  por página, y 0,4–0,5 ms al p50. PostgreSQL y k6 no cambian.
- A 500 req/s son ~32 MB/s sin comprimir frente a ~1,7 MB/s con brotli. Por internet, y en el
  ingress o la CDN, esa es la diferencia que importa; en la red local de la VM casi no se nota.

## Lo que se vio antes (con el Mac ocupado)

Una primera serie alternada a 500 req/s se hizo con procesos de macOS ocupando CPU
(`ApplicationsStorage`, `StorageManagement`), así que solo vale en relativo. Con brotli atendió
464–486 req/s y sin comprimir 354–400 req/s; status-bff usó la misma CPU en los dos casos y
PostgreSQL y k6 menos con brotli. Con la máquina cargada, mover 64 KB por respuesta por la red de
la VM costaba más que comprimirlos.

## Cuellos de botella

| Síntoma                                 | Evidencia                                   | Hipótesis                                 | Arreglo propuesto                                                                                 |
| --------------------------------------- | ------------------------------------------- | ----------------------------------------- | ------------------------------------------------------------------------------------------------- |
| La compresión cuesta CPU en cada página | +15 puntos de CPU de status-bff a 500 req/s | brotli calidad 4 sobre 64 KB por petición | Semana 13: guardar en la caché de Redis la página ya comprimida, para comprimir una vez cada 30 s |

## Cómo reproducir

```sh
just loadtest-up
just loadtest-seed
just loadtest smoke
just loadtest status-page -e ACCEPT_ENCODING= -e DURATION=2m   # sin comprimir
just loadtest status-page -e DURATION=2m                       # con brotli
```
