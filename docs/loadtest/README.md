# Reportes de carga

Un reporte por corrida de k6, escrito desde [TEMPLATE.md](TEMPLATE.md). Cómo se corren las
pruebas: [loadtest/README.md](../../loadtest/README.md). Decisiones:
[ADR 0011](../adr/0011-pruebas-de-carga-k6.md).

- `img/<fecha>-<escenario>/`: capturas de Grafana y del dashboard web de k6.
- `data/<testid>.json`: el `summary.json` de cada corrida, para comparar después. Se copia con
  `just loadtest-keep <testid>`, que le quita `setup_data` (trae los tokens del login).

| Fecha      | Escenario                                                  | Commit    | Veredicto                                                                 | Reporte                                      |
| ---------- | ---------------------------------------------------------- | --------- | ------------------------------------------------------------------------- | -------------------------------------------- |
| 2026-10-19 | api-crud (línea base)                                      | `92928be` | Pasa: p95 44 ms a 135 req/s                                               | [reporte](2026-10-19-api-crud.md)            |
| 2026-10-19 | status-page (línea base sin caché)                         | `92928be` | Pasa: p95 6 ms a 500 req/s; techo ~950 req/s                              | [reporte](2026-10-19-status-page.md)         |
| 2026-10-19 | rate-limit                                                 | `92928be` | Pasa: 600 y 60 OK por ventana, `Retry-After` correcto                     | [reporte](2026-10-19-rate-limit.md)          |
| 2026-10-19 | isolation (y autotest)                                     | `92928be` | Pasa: 0 fugas en 9 365 comprobaciones                                     | [reporte](2026-10-19-isolation.md)           |
| 2026-10-19 | status-page stress, spike y breakpoint                     | `92928be` | SLO hasta ~750–900 req/s, techo ~920; se recupera en segundos             | [reporte](2026-10-19-status-page-limites.md) |
| 2026-10-19 | checker-soak (y sondeo de 30 000 monitores)                | `92928be` | Pasa con 2 000 monitores; techo del checker ~1 000 chequeos/s             | [reporte](2026-10-19-checker-soak.md)        |
| 2026-10-20 | status-page con y sin compresión (brotli), alternadas      | `7a2261d` | Pasa: 19,5 veces menos bytes por ~0,3 ms de CPU por página                | [reporte](2026-10-20-compresion.md)          |
| 2026-10-20 | perfilado de status-bff (status-page a 1 200 req/s)        | `ee90d8e` | 325 → 1 154 req/s con este PR y el filtro de logs estático                | [reporte](2026-10-20-status-bff-perfil.md)   |
| 2026-10-20 | api-crud con el uptime de `uptime_daily` (antes y después) | `acd1902` | Pasa: `/metrics` p50 41 → 21 ms, p95 global 47 → 26 ms                    | [reporte](2026-10-20-api-crud-metrics.md)    |
| 2026-10-20 | checker-soak: sondeo de 30 000 monitores repetido          | `2d82966` | Techo del 19-10: puertos agotados; 1 000/s con 24 % de núcleo             | [reporte](2026-10-20-checker-puertos.md)     |
| 2026-10-21 | status-page en la VPS, con y sin caché de Redis            | `31bbaf3` | Pasa con caché: 50 req/s sin errores (70 % sin ella); SLO hasta 200 req/s | [reporte](2026-10-21-vps-cache.md)           |
